use crate::{resolve, resolve_frame, trace, BacktraceFmt, PrintFmt, Symbol, SymbolName};
use std::ffi::c_void;
use std::fmt;
use std::path::{Path, PathBuf};
use std::prelude::v1::*;

#[cfg(feature = "serde")]
use serde::{Deserialize, Serialize};

/// Representation of an owned and self-contained backtrace.
///
/// This structure can be used to capture a backtrace at various points in a
/// program and later used to inspect what the backtrace was at that time.
///
/// `Backtrace` supports pretty-printing of backtraces through its `Debug`
/// implementation.
///
/// # Required features
///
/// This function requires the `std` feature of the `backtrace` crate to be
/// enabled, and the `std` feature is enabled by default.
#[derive(Clone)]
#[cfg_attr(feature = "serialize-rustc", derive(RustcDecodable, RustcEncodable))]
#[cfg_attr(feature = "serde", derive(Deserialize, Serialize))]
pub struct Backtrace {
    // Frames here are listed from top-to-bottom of the stack
    frames: Vec<BacktraceFrame>,
    // The index we believe is the actual start of the backtrace, omitting
    // frames like `Backtrace::new` and `backtrace::trace`.
    actual_start_index: usize,
}

fn _assert_send_sync() {
    fn _assert<T: Send + Sync>() {}
    _assert::<Backtrace>();
}

/// Captured version of a frame in a backtrace.
///
/// This type is returned as a list from `Backtrace::frames` and represents one
/// stack frame in a captured backtrace.
///
/// # Required features
///
/// This function requires the `std` feature of the `backtrace` crate to be
/// enabled, and the `std` feature is enabled by default.
#[derive(Clone)]
pub struct BacktraceFrame {
    frame: Frame,
    symbols: Option<Vec<BacktraceSymbol>>,
}

#[derive(Clone)]
enum Frame {
    Raw(crate::Frame),
    #[allow(dead_code)]
    Deserialized {
        ip: usize,
        symbol_address: usize,
    },
}

impl Frame {
    fn ip(&self) -> *mut c_void {
        match *self {
            Frame::Raw(ref f) => f.ip(),
            Frame::Deserialized { ip, .. } => ip as *mut c_void,
        }
    }

    fn symbol_address(&self) -> *mut c_void {
        match *self {
            Frame::Raw(ref f) => f.symbol_address(),
            Frame::Deserialized { symbol_address, .. } => symbol_address as *mut c_void,
        }
    }
}

/// Captured version of a symbol in a backtrace.
///
/// This type is returned as a list from `BacktraceFrame::symbols` and
/// represents the metadata for a symbol in a backtrace.
///
/// # Required features
///
/// This function requires the `std` feature of the `backtrace` crate to be
/// enabled, and the `std` feature is enabled by default.
#[derive(Clone)]
#[cfg_attr(feature = "serialize-rustc", derive(RustcDecodable, RustcEncodable))]
#[cfg_attr(feature = "serde", derive(Deserialize, Serialize))]
pub struct BacktraceSymbol {
    name: Option<Vec<u8>>,
    addr: Option<usize>,
    filename: Option<PathBuf>,
    lineno: Option<u32>,
}

impl Backtrace {
    /// Captures a backtrace at the callsite of this function, returning an
    /// owned representation.
    ///
    /// This function is useful for representing a backtrace as an object in
    /// Rust. This returned value can be sent across threads and printed
    /// elsewhere, and the purpose of this value is to be entirely self
    /// contained.
    ///
    /// Note that on some platforms acquiring a full backtrace and resolving it
    /// can be extremely expensive. If the cost is too much for your application
    /// it's recommended to instead use `Backtrace::new_unresolved()` which
    /// avoids the symbol resolution step (which typically takes the longest)
    /// and allows deferring that to a later date.
    ///
    /// # Examples
    ///
    /// ```
    /// use backtrace::Backtrace;
    ///
    /// let current_backtrace = Backtrace::new();
    /// ```
    ///
    /// # Required features
    ///
    /// This function requires the `std` feature of the `backtrace` crate to be
    /// enabled, and the `std` feature is enabled by default.
    #[inline(never)] // want to make sure there's a frame here to remove
    pub fn new() -> Backtrace {
        let mut bt = Self::create(Self::new as usize);
        bt.resolve();
        bt
    }

    /// Similar to `new` except that this does not resolve any symbols, this
    /// simply captures the backtrace as a list of addresses.
    ///
    /// At a later time the `resolve` function can be called to resolve this
    /// backtrace's symbols into readable names. This function exists because
    /// the resolution process can sometimes take a significant amount of time
    /// whereas any one backtrace may only be rarely printed.
    ///
    /// # Examples
    ///
    /// ```
    /// use backtrace::Backtrace;
    ///
    /// let mut current_backtrace = Backtrace::new_unresolved();
    /// println!("{:?}", current_backtrace); // no symbol names
    /// current_backtrace.resolve();
    /// println!("{:?}", current_backtrace); // symbol names now present
    /// ```
    ///
    /// # Required features
    ///
    /// This function requires the `std` feature of the `backtrace` crate to be
    /// enabled, and the `std` feature is enabled by default.
    #[inline(never)] // want to make sure there's a frame here to remove
    pub fn new_unresolved() -> Backtrace {
        Self::create(Self::new_unresolved as usize)
    }

    fn create(ip: usize) -> Backtrace {
        let mut frames = Vec::new();
        let mut actual_start_index = None;
        trace(|frame| {
            frames.push(BacktraceFrame {
                frame: Frame::Raw(frame.clone()),
                symbols: None,
            });

            if frame.symbol_address() as usize == ip && actual_start_index.is_none() {
                actual_start_index = Some(frames.len());
            }
            true
        });

        Backtrace {
            frames,
            actual_start_index: actual_start_index.unwrap_or(0),
        }
    }

    /// Returns the frames from when this backtrace was captured.
    ///
    /// The first entry of this slice is likely the function `Backtrace::new`,
    /// and the last frame is likely something about how this thread or the main
    /// function started.
    ///
    /// # Required features
    ///
    /// This function requires the `std` feature of the `backtrace` crate to be
    /// enabled, and the `std` feature is enabled by default.
    pub fn frames(&self) -> &[BacktraceFrame] {
        &self.frames[self.actual_start_index..]
    }

    /// If this backtrace was created from `new_unresolved` then this function
    /// will resolve all addresses in the backtrace to their symbolic names.
    ///
    /// If this backtrace has been previously resolved or was created through
    /// `new`, this function does nothing.
    ///
    /// # Required features
    ///
    /// This function requires the `std` feature of the `backtrace` crate to be
    /// enabled, and the `std` feature is enabled by default.
    pub fn resolve(&mut self) {
        for frame in self.frames.iter_mut().filter(|f| f.symbols.is_none()) {
            let mut symbols = Vec::new();
            {
                let sym = |symbol: &Symbol| {
                    symbols.push(BacktraceSymbol {
                        name: symbol.name().map(|m| m.as_bytes().to_vec()),
                        addr: symbol.addr().map(|a| a as usize),
                        filename: symbol.filename().map(|m| m.to_owned()),
                        lineno: symbol.lineno(),
                    });
                };
                match frame.frame {
                    Frame::Raw(ref f) => resolve_frame(f, sym),
                    Frame::Deserialized { ip, .. } => {
                        resolve(ip as *mut c_void, sym);
                    }
                }
            }
            frame.symbols = Some(symbols);
        }
    }
}

impl From<Vec<BacktraceFrame>> for Backtrace {
    fn from(frames: Vec<BacktraceFrame>) -> Self {
        Backtrace {
            frames,
            actual_start_index: 0,
        }
    }
}

impl Into<Vec<BacktraceFrame>> for Backtrace {
    fn into(self) -> Vec<BacktraceFrame> {
        self.frames
    }
}

impl BacktraceFrame {
    /// Same as `Frame::ip`
    ///
    /// # Required features
    ///
    /// This function requires the `std` feature of the `backtrace` crate to be
    /// enabled, and the `std` feature is enabled by default.
    pub fn ip(&self) -> *mut c_void {
        self.frame.ip() as *mut c_void
    }

    /// Same as `Frame::symbol_address`
    ///
    /// # Required features
    ///
    /// This function requires the `std` feature of the `backtrace` crate to be
    /// enabled, and the `std` feature is enabled by default.
    pub fn symbol_address(&self) -> *mut c_void {
        self.frame.symbol_address() as *mut c_void
    }

    /// Returns the list of symbols that this frame corresponds to.
    ///
    /// Normally there is only one symbol per frame, but sometimes if a number
    /// of functions are inlined into one frame then multiple symbols will be
    /// returned. The first symbol listed is the "innermost function", whereas
    /// the last symbol is the outermost (last caller).
    ///
    /// Note that if this frame came from an unresolved backtrace then this will
    /// return an empty list.
    ///
    /// # Required features
    ///
    /// This function requires the `std` feature of the `backtrace` crate to be
    /// enabled, and the `std` feature is enabled by default.
    pub fn symbols(&self) -> &[BacktraceSymbol] {
        self.symbols.as_ref().map(|s| &s[..]).unwrap_or(&[])
    }
}

impl BacktraceSymbol {
    /// Same as `Symbol::name`
    ///
    /// # Required features
    ///
    /// This function requires the `std` feature of the `backtrace` crate to be
    /// enabled, and the `std` feature is enabled by default.
    pub fn name(&self) -> Option<SymbolName> {
        self.name.as_ref().map(|s| SymbolName::new(s))
    }

    /// Same as `Symbol::addr`
    ///
    /// # Required features
    ///
    /// This function requires the `std` feature of the `backtrace` crate to be
    /// enabled, and the `std` feature is enabled by default.
    pub fn addr(&self) -> Option<*mut c_void> {
        self.addr.map(|s| s as *mut c_void)
    }

    /// Same as `Symbol::filename`
    ///
    /// # Required features
    ///
    /// This function requires the `std` feature of the `backtrace` crate to be
    /// enabled, and the `std` feature is enabled by default.
    pub fn filename(&self) -> Option<&Path> {
        self.filename.as_ref().map(|p| &**p)
    }

    /// Same as `Symbol::lineno`
    ///
    /// # Required features
    ///
    /// This function requires the `std` feature of the `backtrace` crate to be
    /// enabled, and the `std` feature is enabled by default.
    pub fn lineno(&self) -> Option<u32> {
        self.lineno
    }
}

impl fmt::Debug for Backtrace {
    fn fmt(&self, fmt: &mut fmt::Formatter) -> fmt::Result {
        let full = fmt.alternate();
        let precision = fmt.precision();

        let (frames, style) = if full {
            (&self.frames[..], PrintFmt::Full)
        } else {
            (&self.frames[self.actual_start_index..], PrintFmt::Short)
        };

        let frames = match precision {
            Some(precision) => &frames[..precision],
            None => frames,
        };

        // When printing paths we try to strip the cwd if it exists, otherwise
        // we just print the path as-is. Note that we also only do this for the
        // short format, because if it's full we presumably want to print
        // everything.
        let cwd = std::env::current_dir();
        let mut print_path = move |fmt: &mut fmt::Formatter, path: crate::BytesOrWideString| {
            let path = path.into_path_buf();
            if !full {
                if let Ok(cwd) = &cwd {
                    if let Ok(suffix) = path.strip_prefix(cwd) {
                        return write!(fmt, "{}", &suffix.display());
                    }
                }
            }
            write!(fmt, "{}", &path.display())
        };

        let mut f = BacktraceFmt::new(fmt, style, &mut print_path);
        f.add_context()?;
        for frame in frames {
            f.frame().backtrace_frame(frame)?;
        }
        f.finish()?;
        Ok(())
    }
}

impl Default for Backtrace {
    fn default() -> Backtrace {
        Backtrace::new()
    }
}

impl fmt::Debug for BacktraceFrame {
    fn fmt(&self, fmt: &mut fmt::Formatter) -> fmt::Result {
        fmt.debug_struct("BacktraceFrame")
            .field("ip", &self.ip())
            .field("symbol_address", &self.symbol_address())
            .finish()
    }
}

impl fmt::Debug for BacktraceSymbol {
    fn fmt(&self, fmt: &mut fmt::Formatter) -> fmt::Result {
        fmt.debug_struct("BacktraceSymbol")
            .field("name", &self.name())
            .field("addr", &self.addr())
            .field("filename", &self.filename())
            .field("lineno", &self.lineno())
            .finish()
    }
}

#[cfg(feature = "serialize-rustc")]
mod rustc_serialize_impls {
    use super::*;
    use rustc_serialize::{Decodable, Decoder, Encodable, Encoder};

    #[derive(RustcEncodable, RustcDecodable)]
    struct SerializedFrame {
        ip: usize,
        symbol_address: usize,
        symbols: Option<Vec<BacktraceSymbol>>,
    }

    impl Decodable for BacktraceFrame {
        fn decode<D>(d: &mut D) -> Result<Self, D::Error>
        where
            D: Decoder,
        {
            let frame: SerializedFrame = SerializedFrame::decode(d)?;
            Ok(BacktraceFrame {
                frame: Frame::Deserialized {
                    ip: frame.ip,
                    symbol_address: frame.symbol_address,
                },
                symbols: frame.symbols,
            })
        }
    }

    impl Encodable for BacktraceFrame {
        fn encode<E>(&self, e: &mut E) -> Result<(), E::Error>
        where
            E: Encoder,
        {
            let BacktraceFrame { frame, symbols } = self;
            SerializedFrame {
                ip: frame.ip() as usize,
                symbol_address: frame.symbol_address() as usize,
                symbols: symbols.clone(),
            }
            .encode(e)
        }
    }
}

#[cfg(feature = "serde")]
mod serde_impls {
    extern crate serde;

    use self::serde::de::Deserializer;
    use self::serde::ser::Serializer;
    use self::serde::{Deserialize, Serialize};
    use super::*;

    #[derive(Serialize, Deserialize)]
    struct SerializedFrame {
        ip: usize,
        symbol_address: usize,
        symbols: Option<Vec<BacktraceSymbol>>,
    }

    impl Serialize for BacktraceFrame {
        fn serialize<S>(&self, s: S) -> Result<S::Ok, S::Error>
        where
            S: Serializer,
        {
            let BacktraceFrame { frame, symbols } = self;
            SerializedFrame {
                ip: frame.ip() as usize,
                symbol_address: frame.symbol_address() as usize,
                symbols: symbols.clone(),
            }
            .serialize(s)
        }
    }

    impl<'a> Deserialize<'a> for BacktraceFrame {
        fn deserialize<D>(d: D) -> Result<Self, D::Error>
        where
            D: Deserializer<'a>,
        {
            let frame: SerializedFrame = SerializedFrame::deserialize(d)?;
            Ok(BacktraceFrame {
                frame: Frame::Deserialized {
                    ip: frame.ip,
                    symbol_address: frame.symbol_address,
                },
                symbols: frame.symbols,
            })
        }
    }
}

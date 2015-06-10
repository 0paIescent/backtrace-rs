extern crate backtrace;

use std::str;

fn main() {
    foo();
}

fn foo() { bar() }
fn bar() { baz() }
fn baz() { print() }

#[cfg(target_pointer_width = "32")] const HEX_WIDTH: usize = 10;
#[cfg(target_pointer_width = "64")] const HEX_WIDTH: usize = 20;

fn print() {
    let mut cnt = 0;
    backtrace::trace(&mut |frame| {
        let ip = frame.ip();
        print!("frame #{:<2} - {:#02$x}", cnt, ip as usize, HEX_WIDTH);
        cnt += 1;

        let mut resolved = false;
        backtrace::resolve(frame.ip(), &mut |symbol| {
            if !resolved {
                resolved = true;
            } else {
                print!("\n     ");
            }

            if let Some(name) = symbol.name() {
                if let Ok(s) = str::from_utf8(name) {
                    let mut demangled = String::new();
                    backtrace::demangle(&mut demangled, s).unwrap();
                    print!(" - {}", demangled);
                }
            }
            if let Some(file) = symbol.filename() {
                if let Ok(file) = str::from_utf8(file) {
                    if let Some(l) = symbol.lineno() {
                        print!("\n{:13}{:4$}@ {}:{}", "", "", file, l,
                               HEX_WIDTH);
                    }
                }
            }
            println!("");

        });
        true // keep going
    });
}

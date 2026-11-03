#![windows_subsystem = "windows"]

mod editor;
mod file;
mod fonts;
mod menu;
mod prefs;
mod richedit;
mod theme;
mod themes;
mod toolbar;
mod window;

use std::path::PathBuf;

fn main() {
    let path = std::env::args_os().nth(1).map(PathBuf::from);
    let code = unsafe { window::run(path) };
    std::process::exit(code);
}

pub fn wide(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(std::iter::once(0)).collect()
}

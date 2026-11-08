#![windows_subsystem = "windows"]

mod dialog;
mod diff;
mod docs;
mod editor;
mod file;
mod find;
mod fonts;
mod gdi;
mod languages;
mod menu;
mod notice;
mod prefs;
mod sci;
mod statusbar;
mod syntax_colors;
mod tabs;
mod theme;
mod themes;
mod toolbar;
mod wait;
mod window;

use std::path::PathBuf;

/// Opens a separate z++ window instead of handing the files to the one already running.
const NEW_WINDOW_FLAGS: [&str; 2] = ["--new-window", "-n"];
/// Returns only after the files' tabs are closed, for use as $EDITOR (see wait.rs).
const WAIT_FLAGS: [&str; 2] = ["--wait", "-w"];

const CRASH_LOG: &str = "zpp-crash.txt";

/// A panic aborts z++ (panic = "abort"); before it does, say what happened and where.
fn install_crash_report() {
    std::panic::set_hook(Box::new(|info| {
        let path = std::env::temp_dir().join(CRASH_LOG);
        let report = format!("z++ {} stopped: {info}\n", env!("CARGO_PKG_VERSION"));
        let _ = std::fs::write(&path, &report);
        let text = wide(&format!(
            "{report}\nThis report was saved to {}.",
            path.display()
        ));
        let caption = wide("z++ error");
        unsafe {
            windows_sys::Win32::UI::WindowsAndMessaging::MessageBoxW(
                std::ptr::null_mut(),
                text.as_ptr(),
                caption.as_ptr(),
                windows_sys::Win32::UI::WindowsAndMessaging::MB_OK
                    | windows_sys::Win32::UI::WindowsAndMessaging::MB_ICONERROR,
            );
        }
    }));
}

fn main() {
    install_crash_report();
    let mut is_new_window = false;
    let mut is_wait = false;
    let mut paths = Vec::new();
    for arg in std::env::args_os().skip(1) {
        if NEW_WINDOW_FLAGS.iter().any(|f| arg == *f) {
            is_new_window = true;
        } else if WAIT_FLAGS.iter().any(|f| arg == *f) {
            is_wait = true;
        } else {
            paths.push(PathBuf::from(arg));
        }
    }
    let code = unsafe { window::run(paths, is_new_window, is_wait) };
    std::process::exit(code);
}

pub fn wide(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(std::iter::once(0)).collect()
}

//! `zpp --wait file`: open the file in z++ and return only once its tab is closed, so z++ can be
//! the editor for git, Claude Code and other tools that wait on $EDITOR.
//!
//! The waiting process makes a named event and hands the files plus the event's name to the
//! running z++ (starting one first if needed). z++ sets the event when the last tab opened for
//! that wait closes, or when it exits. The waiter also stops if that z++ process ends.

use std::ffi::c_void;
use std::path::PathBuf;
use std::ptr::null;
use std::time::{Duration, Instant};
use windows_sys::Win32::Foundation::{CloseHandle, HANDLE, HWND, LPARAM, WAIT_FAILED};
use windows_sys::Win32::System::DataExchange::COPYDATASTRUCT;
use windows_sys::Win32::System::Threading::{
    CreateEventW, OpenEventW, OpenProcess, SetEvent, WaitForMultipleObjects, EVENT_MODIFY_STATE,
    INFINITE, PROCESS_SYNCHRONIZE,
};
use windows_sys::Win32::UI::WindowsAndMessaging::{
    AllowSetForegroundWindow, FindWindowW, GetWindowThreadProcessId, SendMessageW, WM_COPYDATA,
};

use crate::wide;

/// WM_COPYDATA tag: the event name, then the files, each NUL-terminated.
pub const COPYDATA_OPEN_WAIT: usize = 0x5A50_5057; // "ZPPW"

/// How long to wait for a z++ we started to show its window.
const START_TIMEOUT: Duration = Duration::from_secs(15);
const START_POLL: Duration = Duration::from_millis(50);

/// Exit codes for the waiting process.
const EXIT_OK: i32 = 0;
const EXIT_NOT_STARTED: i32 = 2;
const EXIT_REFUSED: i32 = 3;
/// z++ itself ended (crash, logoff, killed) before the tab was closed.
const EXIT_EDITOR_ENDED: i32 = 4;

const WAIT_EVENT: u32 = 0; // WAIT_OBJECT_0
const DETACHED_PROCESS: u32 = 0x0000_0008;
const CREATE_BREAKAWAY_FROM_JOB: u32 = 0x0100_0000;

/// Opens `paths` in z++ and blocks until their tabs are closed. Returns the exit code.
pub unsafe fn run(paths: &[PathBuf], class_name: *const u16) -> i32 {
    let Some(window) = find_or_start(class_name) else {
        return EXIT_NOT_STARTED;
    };
    // The time keeps a reused process ID from matching an older waiter's name.
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_nanos());
    let name = format!("Local\\zpp-wait-{}-{nanos}", std::process::id());
    let name_w = wide(&name);
    let event = CreateEventW(null(), 1, 0, name_w.as_ptr());
    if event.is_null() {
        return EXIT_NOT_STARTED;
    }
    let mut pid = 0u32;
    GetWindowThreadProcessId(window, &mut pid);
    let process = OpenProcess(PROCESS_SYNCHRONIZE, 0, pid);
    let mut data: Vec<u16> = name.encode_utf16().chain([0]).collect();
    for p in paths {
        let full = std::path::absolute(p).unwrap_or_else(|_| p.clone());
        data.extend(full.to_string_lossy().encode_utf16());
        data.push(0);
    }
    data.push(0);
    let cds = COPYDATASTRUCT {
        dwData: COPYDATA_OPEN_WAIT,
        cbData: (data.len() * 2) as u32,
        lpData: data.as_ptr() as *mut c_void,
    };
    AllowSetForegroundWindow(pid);
    // An older z++ that does not know this message answers 0.
    let is_accepted = SendMessageW(window, WM_COPYDATA, 0, &cds as *const _ as LPARAM) == 1;
    let code = if !is_accepted {
        EXIT_REFUSED
    } else {
        let handles: Vec<HANDLE> = [event, process]
            .into_iter()
            .filter(|h| !h.is_null())
            .collect();
        match WaitForMultipleObjects(handles.len() as u32, handles.as_ptr(), 0, INFINITE) {
            WAIT_EVENT => EXIT_OK,
            WAIT_FAILED => EXIT_REFUSED,
            _ => EXIT_EDITOR_ENDED,
        }
    };
    CloseHandle(event);
    if !process.is_null() {
        CloseHandle(process);
    }
    code
}

/// The running z++ window, or a new z++ started without files.
unsafe fn find_or_start(class_name: *const u16) -> Option<HWND> {
    let existing = FindWindowW(class_name, null());
    if !existing.is_null() {
        return Some(existing);
    }
    start_detached()?;
    let started = Instant::now();
    while started.elapsed() < START_TIMEOUT {
        std::thread::sleep(START_POLL);
        let hwnd = FindWindowW(class_name, null());
        if !hwnd.is_null() {
            return Some(hwnd);
        }
    }
    None
}

/// Starts a z++ that outlives this waiter: no inherited console or pipes (a caller reading the
/// editor's output would otherwise wait on it), and out of the caller's job when allowed.
fn start_detached() -> Option<()> {
    use std::os::windows::process::CommandExt;
    use std::process::{Command, Stdio};
    let exe = std::env::current_exe().ok()?;
    let spawn = |flags: u32| {
        Command::new(&exe)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .creation_flags(flags)
            .spawn()
    };
    // Breaking away fails in a job that forbids it; then stay in the job.
    spawn(DETACHED_PROCESS | CREATE_BREAKAWAY_FROM_JOB)
        .or_else(|_| spawn(DETACHED_PROCESS))
        .ok()
        .map(|_| ())
}

/// Tells a waiting process its files are done. A waiter that is gone is ignored.
pub unsafe fn release(name: &str) {
    let name = wide(name);
    let event = OpenEventW(EVENT_MODIFY_STATE, 0, name.as_ptr());
    if event.is_null() {
        return;
    }
    SetEvent(event);
    CloseHandle(event);
}

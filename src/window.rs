//! Main window, menus, commands and the message loop.

use std::cell::{Cell, RefCell};
use std::ffi::OsString;
use std::mem::{size_of, zeroed};
use std::os::windows::ffi::{OsStrExt, OsStringExt};
use std::path::{Path, PathBuf};
use std::ptr::{null, null_mut};
use windows_sys::w;
use windows_sys::Win32::Foundation::*;
use windows_sys::Win32::System::LibraryLoader::{GetModuleHandleW, LoadLibraryW};
use windows_sys::Win32::UI::Controls::Dialogs::*;
use crate::richedit::EM_REDO;
use windows_sys::Win32::UI::Input::KeyboardAndMouse::SetFocus;
use windows_sys::Win32::UI::WindowsAndMessaging::*;

use crate::file::{self, Format};
use crate::{editor, prefs, theme, wide};

const APP_NAME: &str = "z++";
const UNTITLED: &str = "Untitled";
const PATH_BUF_LEN: usize = 32 * 1024;
const DEFAULT_WIDTH: i32 = 1000;
const DEFAULT_HEIGHT: i32 = 700;
const FILE_FILTER: &str = "All files\0*.*\0Text files\0*.txt\0\0";

const IDM_NEW: u16 = 100;
const IDM_OPEN: u16 = 101;
const IDM_SAVE: u16 = 102;
const IDM_SAVE_AS: u16 = 103;
const IDM_EXIT: u16 = 104;
const IDM_UNDO: u16 = 110;
const IDM_REDO: u16 = 111;
const IDM_CUT: u16 = 112;
const IDM_COPY: u16 = 113;
const IDM_PASTE: u16 = 114;
const IDM_SELECT_ALL: u16 = 115;
const IDM_WRAP: u16 = 120;
const IDM_DARK: u16 = 121;

struct App {
    main: Cell<HWND>,
    edit: Cell<HWND>,
    is_dark: Cell<bool>,
    is_wrap: Cell<bool>,
    format: Cell<Format>,
    path: RefCell<Option<PathBuf>>,
    paint: RefCell<Option<theme::Paint>>,
}

thread_local! {
    static APP: App = App {
        main: Cell::new(null_mut()),
        edit: Cell::new(null_mut()),
        is_dark: Cell::new(false),
        is_wrap: Cell::new(false),
        format: Cell::new(Format::default()),
        path: RefCell::new(None),
        paint: RefCell::new(None),
    };
}

fn app<R>(f: impl FnOnce(&App) -> R) -> R {
    APP.with(f)
}

pub unsafe fn run(path: Option<PathBuf>) -> i32 {
    LoadLibraryW(w!("Msftedit.dll"));
    let hinst = GetModuleHandleW(null());
    let prefs = prefs::load();
    app(|a| {
        a.is_dark.set(prefs.is_dark);
        a.is_wrap.set(prefs.is_wrap);
        *a.paint.borrow_mut() = Some(theme::Paint::new());
    });
    theme::set_app_mode(prefs.is_dark);

    let class = w!("ZppMainWindow");
    let wc = WNDCLASSEXW {
        cbSize: size_of::<WNDCLASSEXW>() as u32,
        style: CS_HREDRAW | CS_VREDRAW,
        lpfnWndProc: Some(wndproc),
        hInstance: hinst,
        hIcon: LoadIconW(null_mut(), IDI_APPLICATION),
        hCursor: LoadCursorW(null_mut(), IDC_ARROW),
        lpszClassName: class,
        ..zeroed()
    };
    RegisterClassExW(&wc);
    let title = wide(APP_NAME);
    let hwnd = CreateWindowExW(
        0, class, title.as_ptr(), WS_OVERLAPPEDWINDOW | WS_CLIPCHILDREN,
        CW_USEDEFAULT, CW_USEDEFAULT, DEFAULT_WIDTH, DEFAULT_HEIGHT,
        null_mut(), build_menu(), hinst, null(),
    );
    if hwnd.is_null() {
        return 1;
    }
    ShowWindow(hwnd, SW_SHOWDEFAULT);
    if let Some(p) = path {
        load(&p);
    }
    message_loop(hwnd)
}

unsafe fn message_loop(hwnd: HWND) -> i32 {
    let accel = build_accelerators();
    let mut msg: MSG = zeroed();
    while GetMessageW(&mut msg, null_mut(), 0, 0) > 0 {
        if TranslateAcceleratorW(hwnd, accel, &msg) == 0 {
            TranslateMessage(&msg);
            DispatchMessageW(&msg);
        }
    }
    msg.wParam as i32
}

unsafe extern "system" fn wndproc(hwnd: HWND, msg: u32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    match msg {
        WM_CREATE => on_create(hwnd),
        WM_SIZE => {
            let (w, h) = ((lparam & 0xFFFF) as i32, ((lparam >> 16) & 0xFFFF) as i32);
            MoveWindow(app(|a| a.edit.get()), 0, 0, w, h, 1);
            0
        }
        WM_SETFOCUS => {
            SetFocus(app(|a| a.edit.get()));
            0
        }
        WM_COMMAND => on_command(hwnd, wparam, lparam),
        WM_CLOSE => {
            if confirm_discard() {
                DestroyWindow(hwnd);
            }
            0
        }
        WM_QUERYENDSESSION => confirm_discard() as LRESULT,
        WM_DESTROY => {
            app(|a| prefs::save(&prefs::Prefs { is_dark: a.is_dark.get(), is_wrap: a.is_wrap.get() }));
            PostQuitMessage(0);
            0
        }
        theme::WM_UAHDRAWMENU | theme::WM_UAHDRAWMENUITEM if app(|a| a.is_dark.get()) => {
            app(|a| {
                let paint = a.paint.borrow();
                let paint = paint.as_ref().expect("paint initialized in run");
                if msg == theme::WM_UAHDRAWMENU {
                    theme::draw_menu_bar(hwnd, lparam, paint);
                } else {
                    theme::draw_menu_item(lparam, paint);
                }
            });
            0
        }
        WM_NCPAINT | WM_NCACTIVATE => {
            let result = DefWindowProcW(hwnd, msg, wparam, lparam);
            if app(|a| a.is_dark.get()) {
                app(|a| theme::paint_menu_underline(hwnd, a.paint.borrow().as_ref().expect("paint")));
            }
            result
        }
        _ => DefWindowProcW(hwnd, msg, wparam, lparam),
    }
}

unsafe fn on_create(hwnd: HWND) -> LRESULT {
    let hinst = GetModuleHandleW(null());
    let edit = editor::create(hwnd, hinst);
    if edit.is_null() {
        return -1;
    }
    let (is_dark, is_wrap) = app(|a| {
        a.main.set(hwnd);
        a.edit.set(edit);
        (a.is_dark.get(), a.is_wrap.get())
    });
    editor::set_wrap(edit, is_wrap);
    theme::apply(hwnd, edit, is_dark);
    update_checks();
    update_title();
    0
}

unsafe fn on_command(hwnd: HWND, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    let id = (wparam & 0xFFFF) as u16;
    let code = ((wparam >> 16) & 0xFFFF) as u32;
    let edit = app(|a| a.edit.get());
    if lparam != 0 {
        if lparam as HWND == edit && code == EN_CHANGE {
            update_title();
        }
        return 0;
    }
    match id {
        IDM_NEW => cmd_new(),
        IDM_OPEN => cmd_open(),
        IDM_SAVE => {
            cmd_save();
        }
        IDM_SAVE_AS => {
            cmd_save_as();
        }
        IDM_EXIT => {
            SendMessageW(hwnd, WM_CLOSE, 0, 0);
        }
        IDM_UNDO => editor::send(edit, WM_UNDO),
        IDM_REDO => editor::send(edit, EM_REDO),
        IDM_CUT => editor::send(edit, WM_CUT),
        IDM_COPY => editor::send(edit, WM_COPY),
        IDM_PASTE => editor::send(edit, WM_PASTE),
        IDM_SELECT_ALL => editor::select_all(edit),
        IDM_WRAP => toggle_wrap(),
        IDM_DARK => toggle_dark(),
        _ => return DefWindowProcW(hwnd, WM_COMMAND, wparam, lparam),
    }
    0
}

unsafe fn cmd_new() {
    if !confirm_discard() {
        return;
    }
    app(|a| {
        *a.path.borrow_mut() = None;
        a.format.set(Format::default());
        editor::set_text(a.edit.get(), &[]);
    });
    update_title();
}

unsafe fn cmd_open() {
    if !confirm_discard() {
        return;
    }
    if let Some(p) = file_dialog(false) {
        load(&p);
    }
}

unsafe fn load(path: &Path) {
    let bytes = match std::fs::read(path) {
        Ok(b) => b,
        Err(e) => return error_box(&format!("Could not open {}:\n{e}", path.display())),
    };
    let (text, format) = match file::decode(&bytes) {
        Ok(decoded) => decoded,
        Err(why) => return error_box(&format!("z++ will not open {} because {why}.", path.display())),
    };
    app(|a| {
        editor::set_text(a.edit.get(), &text);
        a.format.set(format);
        *a.path.borrow_mut() = Some(path.to_path_buf());
    });
    update_title();
}

unsafe fn cmd_save() -> bool {
    match app(|a| a.path.borrow().clone()) {
        Some(p) => save_to(&p),
        None => cmd_save_as(),
    }
}

unsafe fn cmd_save_as() -> bool {
    match file_dialog(true) {
        Some(p) => save_to(&p),
        None => false,
    }
}

unsafe fn save_to(path: &Path) -> bool {
    let (edit, format) = app(|a| (a.edit.get(), a.format.get()));
    let (bytes, used) = file::encode(&editor::text(edit), format);
    if let Err(e) = std::fs::write(path, &bytes) {
        error_box(&format!("Could not save {}:\n{e}", path.display()));
        return false;
    }
    app(|a| {
        a.format.set(used);
        *a.path.borrow_mut() = Some(path.to_path_buf());
    });
    editor::set_modified(edit, false);
    update_title();
    if used.encoding != format.encoding {
        info_box(&format!(
            "Some characters are not in the {} code page, so the file was saved as {}.",
            file::encoding_label(format.encoding),
            file::encoding_label(used.encoding),
        ));
    }
    true
}

/// True when it is fine to throw away the current text.
unsafe fn confirm_discard() -> bool {
    if !editor::is_modified(app(|a| a.edit.get())) {
        return true;
    }
    let text = wide(&format!("Save changes to {}?", display_name()));
    let caption = wide(APP_NAME);
    let choice = MessageBoxW(app(|a| a.main.get()), text.as_ptr(), caption.as_ptr(), MB_YESNOCANCEL | MB_ICONWARNING);
    match choice {
        IDYES => cmd_save(),
        IDNO => true,
        _ => false,
    }
}

unsafe fn file_dialog(is_save: bool) -> Option<PathBuf> {
    let mut buf = vec![0u16; PATH_BUF_LEN];
    let current = app(|a| a.path.borrow().clone());
    let mut initial_dir: Option<Vec<u16>> = None;
    if let Some(p) = &current {
        if is_save {
            let name: Vec<u16> = p.file_name().unwrap_or_default().encode_wide().collect();
            buf[..name.len().min(PATH_BUF_LEN - 1)].copy_from_slice(&name[..name.len().min(PATH_BUF_LEN - 1)]);
        }
        initial_dir = p.parent().map(|d| d.as_os_str().encode_wide().chain([0]).collect());
    }
    let mut ofn: OPENFILENAMEW = zeroed();
    ofn.lStructSize = size_of::<OPENFILENAMEW>() as u32;
    ofn.hwndOwner = app(|a| a.main.get());
    let filter: Vec<u16> = FILE_FILTER.encode_utf16().collect();
    ofn.lpstrFilter = filter.as_ptr();
    ofn.nFilterIndex = 1;
    ofn.lpstrFile = buf.as_mut_ptr();
    ofn.nMaxFile = PATH_BUF_LEN as u32;
    ofn.lpstrInitialDir = initial_dir.as_ref().map_or(null(), |d| d.as_ptr());
    ofn.Flags = OFN_EXPLORER | OFN_PATHMUSTEXIST | OFN_NOCHANGEDIR;
    let is_ok = if is_save {
        ofn.Flags |= OFN_OVERWRITEPROMPT;
        GetSaveFileNameW(&mut ofn)
    } else {
        ofn.Flags |= OFN_FILEMUSTEXIST;
        GetOpenFileNameW(&mut ofn)
    };
    if is_ok == 0 {
        return None;
    }
    let len = buf.iter().position(|&c| c == 0).unwrap_or(buf.len());
    Some(PathBuf::from(OsString::from_wide(&buf[..len])))
}

unsafe fn toggle_dark() {
    let (main, edit, is_dark) = app(|a| {
        a.is_dark.set(!a.is_dark.get());
        (a.main.get(), a.edit.get(), a.is_dark.get())
    });
    theme::apply(main, edit, is_dark);
    update_checks();
    update_title();
}

unsafe fn toggle_wrap() {
    let (edit, is_wrap) = app(|a| {
        a.is_wrap.set(!a.is_wrap.get());
        (a.edit.get(), a.is_wrap.get())
    });
    editor::set_wrap(edit, is_wrap);
    update_checks();
}

unsafe fn update_checks() {
    let (menu, is_dark, is_wrap) = app(|a| (GetMenu(a.main.get()), a.is_dark.get(), a.is_wrap.get()));
    let flag = |on: bool| if on { MF_CHECKED } else { MF_UNCHECKED };
    CheckMenuItem(menu, IDM_DARK as u32, MF_BYCOMMAND | flag(is_dark));
    CheckMenuItem(menu, IDM_WRAP as u32, MF_BYCOMMAND | flag(is_wrap));
}

fn display_name() -> String {
    app(|a| {
        a.path.borrow().as_ref().and_then(|p| p.file_name()).map_or_else(
            || UNTITLED.to_string(),
            |n| n.to_string_lossy().into_owned(),
        )
    })
}

unsafe fn update_title() {
    let (main, edit) = app(|a| (a.main.get(), a.edit.get()));
    if main.is_null() || edit.is_null() {
        return;
    }
    let marker = if editor::is_modified(edit) { "*" } else { "" };
    let title = wide(&format!("{marker}{} - {APP_NAME}", display_name()));
    SetWindowTextW(main, title.as_ptr());
}

unsafe fn build_menu() -> HMENU {
    let bar = CreateMenu();
    let file = popup(&[
        (IDM_NEW, "&New\tCtrl+N"),
        (IDM_OPEN, "&Open...\tCtrl+O"),
        (IDM_SAVE, "&Save\tCtrl+S"),
        (IDM_SAVE_AS, "Save &As...\tCtrl+Shift+S"),
        (0, ""),
        (IDM_EXIT, "E&xit"),
    ]);
    let edit = popup(&[
        (IDM_UNDO, "&Undo\tCtrl+Z"),
        (IDM_REDO, "&Redo\tCtrl+Y"),
        (0, ""),
        (IDM_CUT, "Cu&t\tCtrl+X"),
        (IDM_COPY, "&Copy\tCtrl+C"),
        (IDM_PASTE, "&Paste\tCtrl+V"),
        (0, ""),
        (IDM_SELECT_ALL, "Select &All\tCtrl+A"),
    ]);
    let view = popup(&[(IDM_WRAP, "&Word Wrap\tAlt+Z"), (IDM_DARK, "&Dark Mode\tCtrl+Shift+D")]);
    for (menu, label) in [(file, "&File"), (edit, "&Edit"), (view, "&View")] {
        let label = wide(label);
        AppendMenuW(bar, MF_POPUP, menu as usize, label.as_ptr());
    }
    bar
}

/// id 0 is a separator.
unsafe fn popup(items: &[(u16, &str)]) -> HMENU {
    let menu = CreatePopupMenu();
    for &(id, label) in items {
        if id == 0 {
            AppendMenuW(menu, MF_SEPARATOR, 0, null());
        } else {
            let label = wide(label);
            AppendMenuW(menu, MF_STRING, id as usize, label.as_ptr());
        }
    }
    menu
}

unsafe fn build_accelerators() -> HACCEL {
    let ctrl = (FVIRTKEY | FCONTROL) as u8;
    let ctrl_shift = (FVIRTKEY | FCONTROL | FSHIFT) as u8;
    let alt = (FVIRTKEY | FALT) as u8;
    let table = [
        ACCEL { fVirt: ctrl, key: b'N' as u16, cmd: IDM_NEW },
        ACCEL { fVirt: ctrl, key: b'O' as u16, cmd: IDM_OPEN },
        ACCEL { fVirt: ctrl, key: b'S' as u16, cmd: IDM_SAVE },
        ACCEL { fVirt: ctrl_shift, key: b'S' as u16, cmd: IDM_SAVE_AS },
        ACCEL { fVirt: ctrl_shift, key: b'D' as u16, cmd: IDM_DARK },
        ACCEL { fVirt: alt, key: b'Z' as u16, cmd: IDM_WRAP },
    ];
    CreateAcceleratorTableW(table.as_ptr(), table.len() as i32)
}

unsafe fn error_box(text: &str) {
    message_box(text, MB_OK | MB_ICONERROR);
}

unsafe fn info_box(text: &str) {
    message_box(text, MB_OK | MB_ICONINFORMATION);
}

unsafe fn message_box(text: &str, style: MESSAGEBOX_STYLE) {
    let text = wide(text);
    let caption = wide(APP_NAME);
    MessageBoxW(app(|a| a.main.get()), text.as_ptr(), caption.as_ptr(), style);
}

//! Main window: layout, the message loop, commands, and the look (theme, font, size).
//! Open files live in docs.rs.

use std::cell::{Cell, RefCell};
use std::ffi::c_void;
use std::mem::{size_of, zeroed};
use std::path::PathBuf;
use std::ptr::{null, null_mut};
use windows_sys::w;
use windows_sys::Win32::Foundation::*;
use windows_sys::Win32::Graphics::Gdi::{
    GetDC, GetDeviceCaps, MonitorFromRect, ReleaseDC, DEFAULT_CHARSET, LOGFONTW, LOGPIXELSY,
    MONITOR_DEFAULTTONULL,
};
use windows_sys::Win32::System::DataExchange::COPYDATASTRUCT;
use windows_sys::Win32::System::LibraryLoader::GetModuleHandleW;
use windows_sys::Win32::UI::Controls::Dialogs::*;
use windows_sys::Win32::UI::Controls::NMHDR;
use windows_sys::Win32::UI::Input::KeyboardAndMouse::SetFocus;
use windows_sys::Win32::UI::Shell::{DragAcceptFiles, DragFinish, DragQueryFileW, HDROP};
use windows_sys::Win32::UI::WindowsAndMessaging::*;

use crate::docs::{self, Doc};
use crate::editor::Span;
use crate::menu::*;
use crate::sci::{self, SCNotification};
use crate::themes::{self, Colors, Syntax};
use crate::{
    dialog, editor, find, findfiles, fonts, menu, notice, prefs, results, statusbar, tabs, theme,
    toolbar, wait, wide,
};

const APP_NAME: &str = "z++";
const CLASS_NAME: *const u16 = w!("ZppMainWindow");
/// Resource ID of the icon in build.rs's .rc file.
const APP_ICON_ID: usize = 1;
const DEFAULT_WIDTH: i32 = 1100;
const DEFAULT_HEIGHT: i32 = 750;
const LICENSES_FILE: &str = "z++ font licenses.txt";
const TENTHS_PER_INCH: u32 = 720;
/// WM_COPYDATA tag for "open these files", sent by a second z++ to the first one.
const COPYDATA_OPEN: usize = 0x5A50;
const WM_APP_OPEN_PENDING: u32 = WM_APP + 5;
/// Checks the active tab's file this often, so changes made by another program (an agent, a
/// build) show up while z++ is in the background.
const DISK_TIMER_ID: usize = 1;
const DISK_POLL_MS: u32 = 1000;
const MAX_PATH_UNITS: usize = 32 * 1024;

pub struct App {
    pub main: Cell<HWND>,
    /// The active tab's editor.
    pub edit: Cell<HWND>,
    pub toolbar: Cell<HWND>,
    pub tab_strip: Cell<HWND>,
    pub status: Cell<HWND>,
    pub notice: Cell<HWND>,
    pub menus: Cell<Menus>,
    pub docs: RefCell<Vec<Doc>>,
    pub active: Cell<usize>,
    pub next_untitled: Cell<u32>,
    /// Bumped whenever colors or font change; each editor is restyled when next shown.
    pub look_gen: Cell<u32>,
    pub is_checking_disk: Cell<bool>,
    pub recent: RefCell<Vec<PathBuf>>,
    /// Files to open, each with the --wait event to set when its tab closes.
    pending_open: RefCell<Vec<(PathBuf, Option<String>)>>,
    theme: Cell<usize>,
    custom: Cell<Colors>,
    last_light: Cell<usize>,
    last_dark: Cell<usize>,
    face: RefCell<String>,
    /// Font size in tenths of a point.
    pub size: Cell<u32>,
    pub is_wrap: Cell<bool>,
    is_toolbar: Cell<bool>,
    is_status_bar: Cell<bool>,
    pub is_line_numbers: Cell<bool>,
    font_list: RefCell<Vec<String>>,
    custom_colors: RefCell<[u32; 16]>,
    paint: RefCell<Option<theme::Paint>>,
    /// The theme shown while the user browses the Theme menu or dropdown, before choosing.
    preview: Cell<Option<usize>>,
}

thread_local! {
    static APP: App = App {
        main: Cell::new(null_mut()),
        edit: Cell::new(null_mut()),
        toolbar: Cell::new(null_mut()),
        tab_strip: Cell::new(null_mut()),
        status: Cell::new(null_mut()),
        notice: Cell::new(null_mut()),
        menus: Cell::new(Menus { bar: null_mut(), recent: null_mut(), encoding: null_mut(), line_endings: null_mut() }),
        docs: RefCell::new(Vec::new()),
        active: Cell::new(0),
        next_untitled: Cell::new(1),
        look_gen: Cell::new(1),
        is_checking_disk: Cell::new(false),
        recent: RefCell::new(Vec::new()),
        pending_open: RefCell::new(Vec::new()),
        theme: Cell::new(0),
        custom: Cell::new(themes::PRESETS[0].colors),
        last_light: Cell::new(0),
        last_dark: Cell::new(0),
        face: RefCell::new(fonts::DEFAULT_FACE.to_string()),
        size: Cell::new(fonts::DEFAULT_SIZE_TENTHS),
        is_wrap: Cell::new(false),
        is_toolbar: Cell::new(true),
        is_status_bar: Cell::new(true),
        is_line_numbers: Cell::new(true),
        font_list: RefCell::new(Vec::new()),
        custom_colors: RefCell::new([0x00FF_FFFF; 16]),
        paint: RefCell::new(None),
        preview: Cell::new(None),
    };
}

pub fn app<R>(f: impl FnOnce(&App) -> R) -> R {
    APP.with(f)
}

/// The colors on screen: the previewed theme while browsing, otherwise the chosen one.
fn colors() -> Colors {
    app(|a| {
        let theme = a.preview.get().unwrap_or(a.theme.get());
        themes::colors_of(theme, a.custom.get())
    })
}

/// Colors, code colors, font face and size (tenths of a point) for styling an editor.
pub fn look_parts() -> (Colors, Syntax, String, u32) {
    let colors = colors();
    let theme = app(|a| a.preview.get().unwrap_or(a.theme.get()));
    (
        colors,
        themes::syntax_of(theme, colors),
        app(|a| a.face.borrow().clone()),
        app(|a| a.size.get()),
    )
}

fn theme_or(name: &str, fallback: &str) -> usize {
    themes::index_of(name)
        .or_else(|| themes::index_of(fallback))
        .unwrap_or(0)
}

/// Starts z++, or hands `paths` to a z++ that is already running unless `is_new_window`.
/// With `is_wait`, hands them over and returns once their tabs are closed.
pub unsafe fn run(paths: Vec<PathBuf>, is_new_window: bool, is_wait: bool) -> i32 {
    if is_wait {
        return wait::run(&paths, CLASS_NAME);
    }
    if !is_new_window && hand_off(&paths) {
        return 0;
    }
    let hinst = GetModuleHandleW(null());
    editor::register(hinst);
    fonts::register_bundled();
    let windows_fonts = fonts::windows_fonts();
    load_prefs(&windows_fonts);
    theme::set_app_mode(themes::is_dark(colors().bg));
    let menus = menu::build(&windows_fonts);
    app(|a| a.menus.set(menus));
    let Some(hwnd) = create_main_window(hinst, menus.bar) else {
        return 1;
    };
    let session = prefs::load_session();
    show_at_saved_place(hwnd, &session);
    let recent = session.recent.clone();
    app(|a| *a.recent.borrow_mut() = recent.clone());
    menu::set_recent(menus.recent, &recent);
    docs::restore_session(&session.files, session.active);
    for path in &paths {
        docs::open_location(path);
    }
    SetTimer(hwnd, DISK_TIMER_ID, DISK_POLL_MS, None);
    message_loop(hwnd)
}

unsafe fn create_main_window(hinst: HINSTANCE, bar: HMENU) -> Option<HWND> {
    let wc = WNDCLASSEXW {
        cbSize: size_of::<WNDCLASSEXW>() as u32,
        style: CS_HREDRAW | CS_VREDRAW,
        lpfnWndProc: Some(wndproc),
        hInstance: hinst,
        hIcon: load_icon(hinst, SM_CXICON, SM_CYICON),
        hIconSm: load_icon(hinst, SM_CXSMICON, SM_CYSMICON),
        hCursor: LoadCursorW(null_mut(), IDC_ARROW),
        lpszClassName: CLASS_NAME,
        ..zeroed()
    };
    RegisterClassExW(&wc);
    let title = wide(APP_NAME);
    let hwnd = CreateWindowExW(
        WS_EX_ACCEPTFILES,
        CLASS_NAME,
        title.as_ptr(),
        WS_OVERLAPPEDWINDOW | WS_CLIPCHILDREN,
        CW_USEDEFAULT,
        CW_USEDEFAULT,
        DEFAULT_WIDTH,
        DEFAULT_HEIGHT,
        null_mut(),
        bar,
        hinst,
        null(),
    );
    (!hwnd.is_null()).then_some(hwnd)
}

/// Sends the files to a running z++ and brings it to the front. False if none is running.
unsafe fn hand_off(paths: &[PathBuf]) -> bool {
    let existing = FindWindowW(CLASS_NAME, null());
    if existing.is_null() {
        return false;
    }
    let mut data: Vec<u16> = Vec::new();
    for p in paths {
        let full = std::path::absolute(p).unwrap_or_else(|_| p.clone());
        data.extend(full.to_string_lossy().encode_utf16());
        data.push(0);
    }
    data.push(0);
    let cds = COPYDATASTRUCT {
        dwData: COPYDATA_OPEN,
        cbData: (data.len() * 2) as u32,
        lpData: data.as_ptr() as *mut c_void,
    };
    let mut pid = 0u32;
    GetWindowThreadProcessId(existing, &mut pid);
    AllowSetForegroundWindow(pid);
    SendMessageW(existing, WM_COPYDATA, 0, &cds as *const _ as LPARAM);
    true
}

/// The app icon from the exe's resources (see build.rs), at the system size for that slot.
unsafe fn load_icon(hinst: HINSTANCE, cx: SYSTEM_METRICS_INDEX, cy: SYSTEM_METRICS_INDEX) -> HICON {
    LoadImageW(
        hinst,
        APP_ICON_ID as *const u16,
        IMAGE_ICON,
        GetSystemMetrics(cx),
        GetSystemMetrics(cy),
        LR_DEFAULTCOLOR,
    ) as HICON
}

/// Last time's size and position, if that spot is still on a monitor.
unsafe fn show_at_saved_place(hwnd: HWND, session: &prefs::Session) {
    let saved = session.window.filter(|rc| {
        rc.right > rc.left
            && rc.bottom > rc.top
            && !MonitorFromRect(rc, MONITOR_DEFAULTTONULL).is_null()
    });
    let Some(rc) = saved else {
        ShowWindow(hwnd, SW_SHOWDEFAULT);
        return;
    };
    let mut wp: WINDOWPLACEMENT = zeroed();
    wp.length = size_of::<WINDOWPLACEMENT>() as u32;
    wp.showCmd = (if session.is_maximized {
        SW_SHOWMAXIMIZED
    } else {
        SW_SHOWNORMAL
    }) as u32;
    wp.rcNormalPosition = rc;
    SetWindowPlacement(hwnd, &wp);
}

unsafe fn load_prefs(windows_fonts: &[&str]) {
    let p = prefs::load();
    let font_list: Vec<String> = windows_fonts
        .iter()
        .map(|f| f.to_string())
        .chain(fonts::BUNDLED.iter().map(|b| b.family.to_string()))
        .collect();
    let face = if fonts::is_available(&p.face) {
        p.face
    } else {
        fonts::DEFAULT_FACE.to_string()
    };
    app(|a| {
        a.theme.set(theme_or(&p.theme, themes::LIGHT));
        a.last_light.set(theme_or(&p.last_light, themes::LIGHT));
        a.last_dark.set(theme_or(&p.last_dark, themes::DARK));
        a.custom.set(p.custom);
        *a.face.borrow_mut() = face;
        a.size.set(fonts::clamp_size(p.size_tenths));
        a.is_wrap.set(p.is_wrap);
        a.is_toolbar.set(p.is_toolbar);
        a.is_status_bar.set(p.is_status_bar);
        a.is_line_numbers.set(p.is_line_numbers);
        *a.font_list.borrow_mut() = font_list;
    });
}

unsafe fn save_prefs() {
    let p = app(|a| prefs::Prefs {
        theme: themes::name_of(a.theme.get()).to_string(),
        last_light: themes::name_of(a.last_light.get()).to_string(),
        last_dark: themes::name_of(a.last_dark.get()).to_string(),
        custom: a.custom.get(),
        face: a.face.borrow().clone(),
        size_tenths: a.size.get(),
        is_wrap: a.is_wrap.get(),
        is_toolbar: a.is_toolbar.get(),
        is_status_bar: a.is_status_bar.get(),
        is_line_numbers: a.is_line_numbers.get(),
    });
    prefs::save(&p);
}

/// Open files and window position, saved while the window still exists.
unsafe fn save_session(hwnd: HWND) {
    let (files, active) = docs::session();
    let mut wp: WINDOWPLACEMENT = zeroed();
    wp.length = size_of::<WINDOWPLACEMENT>() as u32;
    let has_place = GetWindowPlacement(hwnd, &mut wp) != 0;
    prefs::save_session(&prefs::Session {
        files,
        active,
        recent: app(|a| a.recent.borrow().clone()),
        window: has_place.then_some(wp.rcNormalPosition),
        is_maximized: wp.showCmd == SW_SHOWMAXIMIZED as u32
            || (wp.showCmd == SW_SHOWMINIMIZED as u32 && wp.flags & WPF_RESTORETOMAXIMIZED != 0),
    });
}

unsafe fn message_loop(hwnd: HWND) -> i32 {
    let accel = menu::accelerators();
    let mut msg: MSG = zeroed();
    while GetMessageW(&mut msg, null_mut(), 0, 0) > 0 {
        let finder = find::dialog();
        if !finder.is_null() && IsDialogMessageW(finder, &msg) != 0 {
            continue;
        }
        let in_files = findfiles::dialog();
        if !in_files.is_null() && IsDialogMessageW(in_files, &msg) != 0 {
            continue;
        }
        if TranslateAcceleratorW(hwnd, accel, &msg) != 0 {
            continue;
        }
        TranslateMessage(&msg);
        DispatchMessageW(&msg);
    }
    msg.wParam as i32
}

unsafe extern "system" fn wndproc(hwnd: HWND, msg: u32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    match msg {
        WM_CREATE => on_create(hwnd),
        WM_SIZE => {
            layout();
            0
        }
        WM_SETFOCUS => {
            SetFocus(app(|a| a.edit.get()));
            0
        }
        WM_ACTIVATE if (wparam & 0xFFFF) as u32 != WA_INACTIVE => {
            PostMessageW(hwnd, WM_APP_OPEN_PENDING, 1, 0);
            DefWindowProcW(hwnd, msg, wparam, lparam)
        }
        WM_COMMAND => on_command(hwnd, wparam, lparam),
        WM_NOTIFY => {
            on_notify(lparam);
            0
        }
        WM_DROPFILES => {
            open_dropped(wparam as HDROP);
            DragFinish(wparam as HDROP);
            0
        }
        sci::ZPP_WM_FILES_DROPPED => {
            open_dropped(wparam as HDROP); // the editor frees this handle
            0
        }
        WM_COPYDATA => on_copy_data(hwnd, lparam),
        WM_APP_OPEN_PENDING => {
            open_pending(wparam == 1);
            0
        }
        tabs::WM_APP_TAB => {
            on_tab(wparam, lparam as usize);
            0
        }
        findfiles::WM_APP_SEARCH_DONE => {
            findfiles::on_done(wparam, lparam);
            0
        }
        notice::WM_APP_NOTICE => {
            docs::on_notice_button(wparam);
            0
        }
        statusbar::WM_APP_STATUS_CLICK => {
            show_status_menu(hwnd, wparam);
            0
        }
        toolbar::WM_APP_SET_SIZE => {
            set_size(wparam as u32);
            SetFocus(app(|a| a.edit.get()));
            0
        }
        toolbar::WM_APP_PREVIEW_THEME => {
            match wparam {
                toolbar::END_PREVIEW => {
                    end_preview();
                    toolbar::select_theme(app(|a| a.theme.get()));
                }
                index => begin_preview(index),
            }
            0
        }
        WM_MENUSELECT => {
            on_menu_select(wparam, lparam);
            0
        }
        WM_EXITMENULOOP => {
            // Posted, so a theme chosen from the menu is committed before the preview ends.
            PostMessageW(hwnd, toolbar::WM_APP_PREVIEW_THEME, toolbar::END_PREVIEW, 0);
            0
        }
        WM_CLOSE => {
            if docs::confirm_exit() {
                save_session(hwnd);
                DestroyWindow(hwnd);
            }
            0
        }
        WM_QUERYENDSESSION => docs::confirm_exit() as LRESULT,
        // Logoff or shutdown ends the process without WM_DESTROY.
        WM_ENDSESSION if wparam != 0 => {
            docs::release_all_waiters();
            save_session(hwnd);
            save_prefs();
            0
        }
        WM_TIMER if wparam == DISK_TIMER_ID => {
            docs::check_disk(docs::active_index());
            0
        }
        WM_DESTROY => {
            docs::release_all_waiters();
            save_prefs();
            PostQuitMessage(0);
            0
        }
        theme::WM_UAHDRAWMENU | theme::WM_UAHDRAWMENUITEM if themes::is_dark(colors().bg) => {
            app(|a| {
                let paint = a.paint.borrow();
                let Some(paint) = paint.as_ref() else { return };
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
            if themes::is_dark(colors().bg) {
                app(|a| {
                    if let Some(paint) = a.paint.borrow().as_ref() {
                        theme::paint_menu_underline(hwnd, paint);
                    }
                });
            }
            result
        }
        _ => DefWindowProcW(hwnd, msg, wparam, lparam),
    }
}

/// Notifications from the editors.
unsafe fn on_notify(lparam: LPARAM) {
    let hdr = &*(lparam as *const NMHDR);
    if hdr.hwndFrom == results::panel() {
        if hdr.code == sci::SCN_DOUBLECLICK {
            let n = &*(lparam as *const SCNotification);
            results::open_line(n.line.max(0) as usize);
        }
        return;
    }
    let is_active = hdr.hwndFrom == app(|a| a.edit.get());
    match hdr.code {
        sci::SCN_UPDATEUI if is_active => {
            let n = &*(lparam as *const SCNotification);
            if n.updated & (sci::SC_UPDATE_SELECTION | sci::SC_UPDATE_CONTENT) != 0 {
                docs::update_status();
            }
        }
        sci::SCN_ZOOM => {
            docs::fit_margin(hdr.hwndFrom);
            if is_active {
                docs::update_status();
            }
        }
        sci::SCN_SAVEPOINTREACHED | sci::SCN_SAVEPOINTLEFT => docs::on_edit_change(hdr.hwndFrom),
        _ => {}
    }
}

unsafe fn on_create(hwnd: HWND) -> LRESULT {
    let hinst = GetModuleHandleW(null());
    let bar = toolbar::create(hwnd, hinst);
    let strip = tabs::create(hwnd, hinst);
    let status = statusbar::create(hwnd, hinst);
    let notice = notice::create(hwnd, hinst);
    results::create(hwnd, hinst);
    let (is_toolbar, is_status_bar) = app(|a| {
        a.main.set(hwnd);
        a.toolbar.set(bar);
        a.tab_strip.set(strip);
        a.status.set(status);
        a.notice.set(notice);
        (a.is_toolbar.get(), a.is_status_bar.get())
    });
    if !is_toolbar {
        ShowWindow(bar, SW_HIDE);
    }
    if !is_status_bar {
        ShowWindow(status, SW_HIDE);
    }
    DragAcceptFiles(hwnd, 1);
    docs::new_tab();
    apply_look();
    0
}

/// Toolbar, tabs, the active editor, then the status bar, top to bottom.
pub unsafe fn layout() {
    let (main, edit, bar, strip, status, notice, is_toolbar, is_status_bar) = app(|a| {
        (
            a.main.get(),
            a.edit.get(),
            a.toolbar.get(),
            a.tab_strip.get(),
            a.status.get(),
            a.notice.get(),
            a.is_toolbar.get(),
            a.is_status_bar.get(),
        )
    });
    if main.is_null() {
        return;
    }
    let mut rc: RECT = zeroed();
    GetClientRect(main, &mut rc);
    let bar_h = if is_toolbar { toolbar::height() } else { 0 };
    let tabs_h = tabs::height();
    let status_h = if is_status_bar {
        statusbar::height()
    } else {
        0
    };
    MoveWindow(bar, 0, 0, rc.right, bar_h, 1);
    MoveWindow(strip, 0, bar_h, rc.right, tabs_h, 1);
    let notice_h = notice::height();
    MoveWindow(notice, 0, bar_h + tabs_h, rc.right, notice_h, 1);
    let top = bar_h + tabs_h + notice_h;
    let results_h = results::height();
    let edit_h = (rc.bottom - top - status_h - results_h).max(0);
    if !edit.is_null() {
        MoveWindow(edit, 0, top, rc.right, edit_h, 1);
    }
    MoveWindow(results::panel(), 0, top + edit_h, rc.right, results_h, 1);
    MoveWindow(status, 0, rc.bottom - status_h, rc.right, status_h, 1);
}

unsafe fn on_command(hwnd: HWND, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    let id = (wparam & 0xFFFF) as u16;
    let code = ((wparam >> 16) & 0xFFFF) as u32;
    if lparam != 0 {
        if code == EN_CHANGE {
            docs::on_edit_change(lparam as HWND);
        }
        return 0;
    }
    let edit = app(|a| a.edit.get());
    match id {
        IDM_NEW => {
            docs::new_tab();
        }
        IDM_OPEN => docs::cmd_open(),
        IDM_SAVE => {
            docs::save(docs::active_index());
        }
        IDM_SAVE_AS => {
            docs::save_as(docs::active_index());
        }
        IDM_SAVE_ALL => docs::save_all(),
        IDM_CLOSE_TAB => {
            docs::close_tab(docs::active_index());
        }
        IDM_NEXT_TAB => docs::cycle(1),
        IDM_PREV_TAB => docs::cycle(-1),
        IDM_CLEAR_RECENT => docs::clear_recent(),
        IDM_EXIT => {
            SendMessageW(hwnd, WM_CLOSE, 0, 0);
            return 0;
        }
        IDM_UNDO => editor::undo(edit),
        IDM_REDO => editor::redo(edit),
        IDM_CUT => editor::cut(edit),
        IDM_COPY => editor::copy(edit),
        IDM_PASTE => editor::paste(edit),
        IDM_SELECT_ALL => editor::select_all(edit),
        IDM_FIND_IN_FILES => {
            findfiles::open(hwnd, edit);
            return 0;
        }
        IDM_RESULTS => {
            let is_shown = !results::is_shown();
            if !is_shown {
                findfiles::cancel();
            }
            results::show(is_shown);
            layout();
        }
        IDM_FIND | IDM_REPLACE => {
            find::open(hwnd, edit, id == IDM_REPLACE);
            return 0;
        }
        IDM_FIND_NEXT => find::find_again(hwnd, edit, true),
        IDM_FIND_PREV => find::find_again(hwnd, edit, false),
        IDM_GOTO => go_to_line(hwnd, edit),
        IDM_EOL_CRLF => docs::set_line_endings(false),
        IDM_EOL_LF => docs::set_line_endings(true),
        IDM_WRAP => toggle_wrap(),
        IDM_TOGGLE_DARK => toggle_dark(),
        IDM_TEXT_COLOR => pick_color(true),
        IDM_BACK_COLOR => pick_color(false),
        IDM_TOOLBAR => toggle_bar(true),
        IDM_STATUS_BAR => toggle_bar(false),
        IDM_LINE_NUMBERS => toggle_line_numbers(),
        IDM_FONT_DIALOG => font_dialog(),
        IDM_FONT_BIGGER => set_size(app(|a| a.size.get()) + fonts::SIZE_STEP_TENTHS),
        IDM_FONT_SMALLER => set_size(app(|a| a.size.get()).saturating_sub(fonts::SIZE_STEP_TENTHS)),
        IDM_FONT_RESET => set_size(fonts::DEFAULT_SIZE_TENTHS),
        IDM_LICENSES => show_licenses(),
        IDM_ABOUT => about(),
        _ => {
            if !on_list_command(id) {
                return DefWindowProcW(hwnd, WM_COMMAND, wparam, lparam);
            }
        }
    }
    SetFocus(app(|a| a.edit.get()));
    0
}

/// Commands whose IDs come from lists: themes, fonts, recent files, encodings.
unsafe fn on_list_command(id: u16) -> bool {
    if let Some(index) = menu::theme_index(id) {
        set_theme(index);
    } else if let Some(index) = menu::recent_index(id) {
        docs::open_recent(index);
    } else if let Some(encoding) = menu::encoding_for(id) {
        docs::set_encoding(encoding);
    } else if let Some(language) = menu::language_for(id) {
        docs::set_language(docs::active_index(), language);
    } else {
        let count = app(|a| a.font_list.borrow().len());
        let Some(index) = menu::font_index(id, count) else {
            return false;
        };
        let face = app(|a| a.font_list.borrow()[index].clone());
        set_face(face);
    }
    true
}

unsafe fn on_tab(action: usize, index: usize) {
    match action {
        tabs::TAB_SELECT => docs::activate(index),
        tabs::TAB_CLOSE => {
            docs::close_tab(index);
        }
        tabs::TAB_NEW => {
            docs::new_tab();
        }
        _ => {}
    }
}

/// Clicking the encoding or line endings in the status bar opens that part of the Format menu.
unsafe fn show_status_menu(hwnd: HWND, part: usize) {
    let menus = app(|a| a.menus.get());
    let menu = if part == statusbar::PART_ENCODING {
        menus.encoding
    } else {
        menus.line_endings
    };
    let mut pt = POINT { x: 0, y: 0 };
    GetCursorPos(&mut pt);
    TrackPopupMenu(
        menu,
        TPM_RIGHTALIGN | TPM_BOTTOMALIGN,
        pt.x,
        pt.y,
        0,
        hwnd,
        null(),
    );
}

unsafe fn go_to_line(hwnd: HWND, edit: HWND) {
    let lines = editor::line_count(edit);
    let (current, _) = editor::caret_line_col(edit);
    let label = format!("Line number (1 to {lines}):");
    if let Some(line) = dialog::ask_number(hwnd, "Go to Line", &label, current, lines) {
        editor::select(edit, Span::at(editor::line_start(edit, line - 1)));
    }
}

/// Queues the files in a drop and opens them from a posted message, so Explorer's drag is not
/// held up by any dialog z++ might show. The caller frees the handle (or the editor does).
unsafe fn open_dropped(drop: HDROP) {
    let count = DragQueryFileW(drop, u32::MAX, null_mut(), 0);
    let mut paths = Vec::new();
    for i in 0..count {
        let mut buf = vec![0u16; MAX_PATH_UNITS];
        let len = DragQueryFileW(drop, i, buf.as_mut_ptr(), buf.len() as u32) as usize;
        let path = PathBuf::from(String::from_utf16_lossy(&buf[..len]));
        if path.is_file() {
            paths.push(path);
        }
    }
    let main = app(|a| {
        a.pending_open
            .borrow_mut()
            .extend(paths.into_iter().map(|p| (p, None)));
        a.main.get()
    });
    PostMessageW(main, WM_APP_OPEN_PENDING, 0, 0);
}

/// Files sent by a second z++ (for example from Explorer's Open with). Queued and opened from
/// a posted message, so the sender is not kept waiting on any dialog z++ might show.
unsafe fn on_copy_data(hwnd: HWND, lparam: LPARAM) -> LRESULT {
    let cds = &*(lparam as *const COPYDATASTRUCT);
    let is_wait = cds.dwData == wait::COPYDATA_OPEN_WAIT;
    if !(is_wait || cds.dwData == COPYDATA_OPEN) || cds.lpData.is_null() || cds.cbData < 2 {
        return 0;
    }
    let units = std::slice::from_raw_parts(cds.lpData as *const u16, cds.cbData as usize / 2);
    let mut strings = units
        .split(|&c| c == 0)
        .filter(|s| !s.is_empty())
        .map(String::from_utf16_lossy);
    // A --wait sender puts its event name first; every file it sends carries it.
    let waiter = if is_wait { strings.next() } else { None };
    let paths: Vec<(PathBuf, Option<String>)> = strings
        .map(|s| (PathBuf::from(s), waiter.clone()))
        .collect();
    if paths.is_empty() {
        if let Some(w) = waiter {
            wait::release(&w);
        }
    }
    app(|a| a.pending_open.borrow_mut().extend(paths));
    if IsIconic(hwnd) != 0 {
        ShowWindow(hwnd, SW_RESTORE);
    }
    SetForegroundWindow(hwnd);
    PostMessageW(hwnd, WM_APP_OPEN_PENDING, 0, 0);
    1
}

/// Opens files handed over by another z++, then checks the active tab against the disk.
unsafe fn open_pending(is_activation: bool) {
    let pending: Vec<(PathBuf, Option<String>)> =
        app(|a| a.pending_open.borrow_mut().drain(..).collect());
    for (path, waiter) in &pending {
        let index = docs::open_location(path);
        if let Some(waiter) = waiter {
            match index {
                Some(index) => docs::add_waiter(index, waiter),
                None => wait::release(waiter),
            }
        }
    }
    if is_activation && pending.is_empty() {
        docs::check_disk(docs::active_index());
    }
}

/// Applies the current colors and font everywhere, then refreshes the menus and toolbar.
unsafe fn apply_look() {
    paint_look();
    sync_controls();
}

/// Repaints the window chrome, the active editor and the strips in the current colors without
/// touching the menus or dropdown contents, so it is safe while a menu or dropdown is open.
/// Other tabs are restyled when they are next shown (docs::activate).
unsafe fn paint_look() {
    let (colors, syntax, face, size) = look_parts();
    let language = docs::language_of(docs::active_index());
    let (main, edit, gen) = app(|a| {
        let gen = a.look_gen.get() + 1;
        a.look_gen.set(gen);
        (a.main.get(), a.edit.get(), gen)
    });
    let paint = theme::Paint::new(colors);
    app(|a| *a.paint.borrow_mut() = Some(paint));
    let look = theme::Look {
        colors,
        syntax,
        face: &face,
        size_tenths: size,
    };
    theme::apply(main, edit, &look, language);
    if !results::panel().is_null() {
        theme::style_editor(results::panel(), &look, None);
        results::apply_syntax(&syntax);
    }
    app(|a| {
        if let Some(doc) = a.docs.borrow_mut().get_mut(a.active.get()) {
            doc.look_gen = gen;
        }
    });
    if !edit.is_null() {
        docs::fit_margin(edit);
    }
    toolbar::set_colors(colors);
    tabs::set_colors(colors);
    statusbar::set_colors(colors);
    notice::set_colors(colors);
}

/// Hovering a theme in View > Theme previews it; hovering anything else restores the theme.
unsafe fn on_menu_select(wparam: WPARAM, lparam: LPARAM) {
    let id = (wparam & 0xFFFF) as u16;
    let flags = ((wparam >> 16) & 0xFFFF) as u32;
    let is_closed = flags == 0xFFFF && lparam == 0;
    let theme = menu::theme_index(id).filter(|_| !is_closed && flags & MF_POPUP == 0);
    match theme {
        Some(index) => begin_preview(index),
        None => end_preview(),
    }
}

unsafe fn begin_preview(index: usize) {
    if index > themes::PRESETS.len() || app(|a| a.preview.replace(Some(index))) == Some(index) {
        return;
    }
    paint_look();
}

unsafe fn end_preview() {
    if app(|a| a.preview.take()).is_some() {
        paint_look();
    }
}

pub unsafe fn sync_menu_checks() {
    let format = docs::active_format();
    let language = app(|a| a.docs.borrow().get(a.active.get()).and_then(|d| d.language));
    let is_results_shown = results::is_shown();
    app(|a| {
        let font_list = a.font_list.borrow();
        let face = a.face.borrow();
        menu::update_checks(
            a.menus.get().bar,
            &Checks {
                theme: a.theme.get(),
                font_list: &font_list,
                face: &face,
                is_wrap: a.is_wrap.get(),
                is_toolbar: a.is_toolbar.get(),
                is_status_bar: a.is_status_bar.get(),
                is_line_numbers: a.is_line_numbers.get(),
                is_results: is_results_shown,
                format,
                language,
            },
        );
    });
}

unsafe fn sync_controls() {
    sync_menu_checks();
    let colors = colors();
    app(|a| {
        let font_list = a.font_list.borrow();
        let face = a.face.borrow();
        toolbar::sync(&toolbar::State {
            colors,
            custom: a.custom.get(),
            theme: a.theme.get(),
            font_list: &font_list,
            face: &face,
            size_tenths: a.size.get(),
            is_wrap: a.is_wrap.get(),
        });
    });
}

unsafe fn set_theme(index: usize) {
    app(|a| {
        a.preview.set(None);
        a.theme.set(index);
    });
    let is_dark = themes::is_dark(colors().bg);
    app(|a| {
        if is_dark {
            a.last_dark.set(index)
        } else {
            a.last_light.set(index)
        }
    });
    apply_look();
}

/// Switches to the last light theme used, or the last dark one.
unsafe fn toggle_dark() {
    let (is_dark, light, dark, custom) = app(|a| {
        (
            themes::is_dark(themes::colors_of(a.theme.get(), a.custom.get()).bg),
            a.last_light.get(),
            a.last_dark.get(),
            a.custom.get(),
        )
    });
    let mut target = if is_dark { light } else { dark };
    if themes::is_dark(themes::colors_of(target, custom).bg) == is_dark {
        target = theme_or(
            if is_dark { themes::LIGHT } else { themes::DARK },
            themes::LIGHT,
        );
    }
    set_theme(target);
}

/// Opens the system color picker for the text or background color; the result is the Custom theme.
unsafe fn pick_color(is_text: bool) {
    let current = colors();
    let mut custom_colors = app(|a| *a.custom_colors.borrow());
    let mut cc: CHOOSECOLORW = zeroed();
    cc.lStructSize = size_of::<CHOOSECOLORW>() as u32;
    cc.hwndOwner = app(|a| a.main.get());
    cc.rgbResult = if is_text { current.fg } else { current.bg };
    cc.lpCustColors = custom_colors.as_mut_ptr();
    cc.Flags = CC_RGBINIT | CC_FULLOPEN | CC_ANYCOLOR;
    if ChooseColorW(&mut cc) == 0 {
        return;
    }
    let picked = if is_text {
        Colors {
            fg: cc.rgbResult,
            ..current
        }
    } else {
        Colors {
            bg: cc.rgbResult,
            ..current
        }
    };
    app(|a| {
        *a.custom_colors.borrow_mut() = custom_colors;
        a.custom.set(picked);
    });
    set_theme(themes::PRESETS.len());
}

unsafe fn set_face(face: String) {
    app(|a| *a.face.borrow_mut() = face);
    apply_look();
}

unsafe fn set_size(tenths: u32) {
    app(|a| a.size.set(fonts::clamp_size(tenths)));
    apply_look();
}

/// The system font dialog, for any installed font. Bundled fonts are in the Font menu instead,
/// because fonts loaded from memory are private to z++ and the dialog cannot list them.
unsafe fn font_dialog() {
    let (main, face, size) = app(|a| (a.main.get(), a.face.borrow().clone(), a.size.get()));
    let screen = GetDC(null_mut());
    let dpi = GetDeviceCaps(screen, LOGPIXELSY as _) as u32;
    ReleaseDC(null_mut(), screen);
    let mut lf: LOGFONTW = zeroed();
    lf.lfHeight = -(((size * dpi + TENTHS_PER_INCH / 2) / TENTHS_PER_INCH) as i32);
    lf.lfCharSet = DEFAULT_CHARSET;
    let units: Vec<u16> = face.encode_utf16().take(lf.lfFaceName.len() - 1).collect();
    lf.lfFaceName[..units.len()].copy_from_slice(&units);
    let mut cf: CHOOSEFONTW = zeroed();
    cf.lStructSize = size_of::<CHOOSEFONTW>() as u32;
    cf.hwndOwner = main;
    cf.lpLogFont = &mut lf;
    cf.iPointSize = size as i32;
    cf.Flags = CF_INITTOLOGFONTSTRUCT
        | CF_NOVERTFONTS
        | CF_FORCEFONTEXIST
        | CF_NOSCRIPTSEL
        | CF_SCREENFONTS;
    if ChooseFontW(&mut cf) == 0 {
        sync_controls();
        return;
    }
    let len = lf
        .lfFaceName
        .iter()
        .position(|&c| c == 0)
        .unwrap_or(lf.lfFaceName.len());
    app(|a| {
        *a.face.borrow_mut() = String::from_utf16_lossy(&lf.lfFaceName[..len]);
        a.size.set(fonts::clamp_size(cf.iPointSize.max(0) as u32));
    });
    apply_look();
}

/// Word wrap applies to every tab.
unsafe fn toggle_wrap() {
    let is_wrap = app(|a| {
        a.is_wrap.set(!a.is_wrap.get());
        a.is_wrap.get()
    });
    let edits: Vec<HWND> = app(|a| a.docs.borrow().iter().map(|d| d.edit).collect());
    for edit in edits {
        editor::set_wrap(edit, is_wrap);
    }
    sync_controls();
}

unsafe fn toggle_line_numbers() {
    let edits: Vec<HWND> = app(|a| {
        a.is_line_numbers.set(!a.is_line_numbers.get());
        a.docs.borrow().iter().map(|d| d.edit).collect()
    });
    for edit in edits {
        docs::fit_margin(edit);
    }
    sync_menu_checks();
}

/// Shows or hides the toolbar (`is_toolbar`) or the status bar.
unsafe fn toggle_bar(is_toolbar: bool) {
    let (hwnd, is_shown) = app(|a| {
        let (flag, hwnd) = if is_toolbar {
            (&a.is_toolbar, a.toolbar.get())
        } else {
            (&a.is_status_bar, a.status.get())
        };
        flag.set(!flag.get());
        (hwnd, flag.get())
    });
    ShowWindow(hwnd, if is_shown { SW_SHOW } else { SW_HIDE });
    layout();
    sync_menu_checks();
}

/// Writes the bundled font licenses to a temp file and opens it in a tab.
unsafe fn show_licenses() {
    let path = std::env::temp_dir().join(LICENSES_FILE);
    if let Err(e) = std::fs::write(&path, fonts::licenses_text()) {
        return error_box(&format!("Could not write {}:\n{e}", path.display()));
    }
    docs::open_path(&path);
}

unsafe fn about() {
    info_box(&format!(
        "{APP_NAME} {}\nA small, fast plain text editor.\n\n{} fonts bundled; see Help > Font Licenses.",
        env!("CARGO_PKG_VERSION"),
        fonts::BUNDLED.len(),
    ));
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Answer {
    Yes,
    No,
    Cancel,
}

pub unsafe fn ask_yes_no_cancel(text: &str) -> Answer {
    match message_box(text, MB_YESNOCANCEL | MB_ICONWARNING) {
        IDYES => Answer::Yes,
        IDNO => Answer::No,
        _ => Answer::Cancel,
    }
}

pub unsafe fn error_box(text: &str) {
    message_box(text, MB_OK | MB_ICONERROR);
}

pub unsafe fn info_box(text: &str) {
    message_box(text, MB_OK | MB_ICONINFORMATION);
}

unsafe fn message_box(text: &str, style: MESSAGEBOX_STYLE) -> MESSAGEBOX_RESULT {
    let text = wide(text);
    let caption = wide(APP_NAME);
    MessageBoxW(
        app(|a| a.main.get()),
        text.as_ptr(),
        caption.as_ptr(),
        style,
    )
}

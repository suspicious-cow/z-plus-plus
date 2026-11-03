//! Main window, commands and the message loop.

use std::cell::{Cell, RefCell};
use std::ffi::OsString;
use std::mem::{size_of, zeroed};
use std::os::windows::ffi::{OsStrExt, OsStringExt};
use std::path::{Path, PathBuf};
use std::ptr::{null, null_mut};
use windows_sys::w;
use windows_sys::Win32::Foundation::*;
use windows_sys::Win32::Graphics::Gdi::{
    GetDC, GetDeviceCaps, ReleaseDC, DEFAULT_CHARSET, LOGFONTW, LOGPIXELSY,
};
use windows_sys::Win32::System::LibraryLoader::{GetModuleHandleW, LoadLibraryW};
use windows_sys::Win32::UI::Controls::Dialogs::*;
use windows_sys::Win32::UI::Input::KeyboardAndMouse::SetFocus;
use windows_sys::Win32::UI::WindowsAndMessaging::*;

use crate::file::{self, Format};
use crate::menu::*;
use crate::richedit::EM_REDO;
use crate::themes::{self, Colors};
use crate::{editor, fonts, menu, prefs, theme, toolbar, wide};

const APP_NAME: &str = "z++";
/// Resource ID of the icon in build.rs's .rc file.
const APP_ICON_ID: usize = 1;
const UNTITLED: &str = "Untitled";
const PATH_BUF_LEN: usize = 32 * 1024;
const DEFAULT_WIDTH: i32 = 1100;
const DEFAULT_HEIGHT: i32 = 750;
const FILE_FILTER: &str = "All files\0*.*\0Text files\0*.txt\0\0";
const LICENSES_FILE: &str = "z++ font licenses.txt";
const TENTHS_PER_INCH: u32 = 720;

struct App {
    main: Cell<HWND>,
    edit: Cell<HWND>,
    toolbar: Cell<HWND>,
    theme: Cell<usize>,
    custom: Cell<Colors>,
    last_light: Cell<usize>,
    last_dark: Cell<usize>,
    face: RefCell<String>,
    size: Cell<u32>,
    is_wrap: Cell<bool>,
    is_toolbar: Cell<bool>,
    font_list: RefCell<Vec<String>>,
    custom_colors: RefCell<[u32; 16]>,
    format: Cell<Format>,
    path: RefCell<Option<PathBuf>>,
    paint: RefCell<Option<theme::Paint>>,
    /// The theme shown while the user browses the Theme menu or dropdown, before choosing.
    preview: Cell<Option<usize>>,
}

thread_local! {
    static APP: App = App {
        main: Cell::new(null_mut()),
        edit: Cell::new(null_mut()),
        toolbar: Cell::new(null_mut()),
        theme: Cell::new(0),
        custom: Cell::new(themes::PRESETS[0].colors),
        last_light: Cell::new(0),
        last_dark: Cell::new(1),
        face: RefCell::new(fonts::DEFAULT_FACE.to_string()),
        size: Cell::new(fonts::DEFAULT_SIZE_TENTHS),
        is_wrap: Cell::new(false),
        is_toolbar: Cell::new(true),
        font_list: RefCell::new(Vec::new()),
        custom_colors: RefCell::new([0x00FF_FFFF; 16]),
        format: Cell::new(Format::default()),
        path: RefCell::new(None),
        paint: RefCell::new(None),
        preview: Cell::new(None),
    };
}

fn app<R>(f: impl FnOnce(&App) -> R) -> R {
    APP.with(f)
}

/// The colors on screen: the previewed theme while browsing, otherwise the chosen one.
fn colors() -> Colors {
    app(|a| {
        let theme = a.preview.get().unwrap_or(a.theme.get());
        themes::colors_of(theme, a.custom.get())
    })
}

fn theme_or(name: &str, fallback: &str) -> usize {
    themes::index_of(name)
        .or_else(|| themes::index_of(fallback))
        .unwrap_or(0)
}

pub unsafe fn run(path: Option<PathBuf>) -> i32 {
    LoadLibraryW(w!("Msftedit.dll"));
    fonts::register_bundled();
    let hinst = GetModuleHandleW(null());
    let windows_fonts = fonts::windows_fonts();
    load_prefs(&windows_fonts);
    theme::set_app_mode(themes::is_dark(colors().bg));

    let class = w!("ZppMainWindow");
    let wc = WNDCLASSEXW {
        cbSize: size_of::<WNDCLASSEXW>() as u32,
        style: CS_HREDRAW | CS_VREDRAW,
        lpfnWndProc: Some(wndproc),
        hInstance: hinst,
        hIcon: load_icon(hinst, SM_CXICON, SM_CYICON),
        hIconSm: load_icon(hinst, SM_CXSMICON, SM_CYSMICON),
        hCursor: LoadCursorW(null_mut(), IDC_ARROW),
        lpszClassName: class,
        ..zeroed()
    };
    RegisterClassExW(&wc);
    let title = wide(APP_NAME);
    let hwnd = CreateWindowExW(
        0,
        class,
        title.as_ptr(),
        WS_OVERLAPPEDWINDOW | WS_CLIPCHILDREN,
        CW_USEDEFAULT,
        CW_USEDEFAULT,
        DEFAULT_WIDTH,
        DEFAULT_HEIGHT,
        null_mut(),
        menu::build(&windows_fonts),
        hinst,
        null(),
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
    });
    prefs::save(&p);
}

unsafe fn message_loop(hwnd: HWND) -> i32 {
    let accel = menu::accelerators();
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
            layout();
            0
        }
        WM_SETFOCUS => {
            SetFocus(app(|a| a.edit.get()));
            0
        }
        WM_COMMAND => on_command(hwnd, wparam, lparam),
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
            if confirm_discard() {
                DestroyWindow(hwnd);
            }
            0
        }
        WM_QUERYENDSESSION => confirm_discard() as LRESULT,
        WM_DESTROY => {
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

unsafe fn on_create(hwnd: HWND) -> LRESULT {
    let hinst = GetModuleHandleW(null());
    let edit = editor::create(hwnd, hinst);
    if edit.is_null() {
        return -1;
    }
    let bar = toolbar::create(hwnd, hinst);
    let (is_wrap, is_toolbar) = app(|a| {
        a.main.set(hwnd);
        a.edit.set(edit);
        a.toolbar.set(bar);
        (a.is_wrap.get(), a.is_toolbar.get())
    });
    if !is_toolbar {
        ShowWindow(bar, SW_HIDE);
    }
    editor::set_wrap(edit, is_wrap);
    apply_look();
    update_title();
    0
}

unsafe fn layout() {
    let (main, edit, bar, is_toolbar) = app(|a| {
        (
            a.main.get(),
            a.edit.get(),
            a.toolbar.get(),
            a.is_toolbar.get(),
        )
    });
    if edit.is_null() {
        return;
    }
    let mut rc: RECT = zeroed();
    GetClientRect(main, &mut rc);
    let bar_h = if is_toolbar { toolbar::height() } else { 0 };
    MoveWindow(bar, 0, 0, rc.right, bar_h, 1);
    MoveWindow(edit, 0, bar_h, rc.right, (rc.bottom - bar_h).max(0), 1);
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
            return 0;
        }
        IDM_UNDO => editor::send(edit, WM_UNDO),
        IDM_REDO => editor::send(edit, EM_REDO),
        IDM_CUT => editor::send(edit, WM_CUT),
        IDM_COPY => editor::send(edit, WM_COPY),
        IDM_PASTE => editor::send(edit, WM_PASTE),
        IDM_SELECT_ALL => editor::select_all(edit),
        IDM_WRAP => toggle_wrap(),
        IDM_TOGGLE_DARK => toggle_dark(),
        IDM_TEXT_COLOR => pick_color(true),
        IDM_BACK_COLOR => pick_color(false),
        IDM_TOOLBAR => toggle_toolbar(),
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
    SetFocus(edit);
    0
}

/// Theme and font commands, whose IDs come from lists. False when `id` is neither.
unsafe fn on_list_command(id: u16) -> bool {
    if let Some(index) = menu::theme_index(id) {
        set_theme(index);
        return true;
    }
    let count = app(|a| a.font_list.borrow().len());
    let Some(index) = menu::font_index(id, count) else {
        return false;
    };
    let face = app(|a| a.font_list.borrow()[index].clone());
    set_face(face);
    true
}

/// Applies the current colors and font everywhere, then refreshes the menus and toolbar.
unsafe fn apply_look() {
    paint_look();
    sync_controls();
}

/// Repaints the window, editor and toolbar in the current colors without touching the menus or
/// the dropdown contents, so it is safe while a menu or dropdown is open.
unsafe fn paint_look() {
    let colors = colors();
    let (main, edit, face, size) = app(|a| {
        (
            a.main.get(),
            a.edit.get(),
            a.face.borrow().clone(),
            a.size.get(),
        )
    });
    let paint = theme::Paint::new(colors);
    app(|a| *a.paint.borrow_mut() = Some(paint));
    theme::apply(
        main,
        edit,
        &theme::Look {
            colors,
            face: &face,
            size_tenths: size,
        },
    );
    toolbar::set_colors(colors);
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

unsafe fn sync_controls() {
    let colors = colors();
    app(|a| {
        let font_list = a.font_list.borrow();
        let face = a.face.borrow();
        let bar = GetMenu(a.main.get());
        menu::update_checks(
            bar,
            a.theme.get(),
            &font_list,
            &face,
            a.is_wrap.get(),
            a.is_toolbar.get(),
        );
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

unsafe fn toggle_wrap() {
    let (edit, is_wrap) = app(|a| {
        a.is_wrap.set(!a.is_wrap.get());
        (a.edit.get(), a.is_wrap.get())
    });
    editor::set_wrap(edit, is_wrap);
    sync_controls();
}

unsafe fn toggle_toolbar() {
    let (bar, is_toolbar) = app(|a| {
        a.is_toolbar.set(!a.is_toolbar.get());
        (a.toolbar.get(), a.is_toolbar.get())
    });
    ShowWindow(bar, if is_toolbar { SW_SHOW } else { SW_HIDE });
    layout();
    sync_controls();
}

/// Writes the bundled font licenses to a temp file and opens it in a second z++ window.
unsafe fn show_licenses() {
    let path = std::env::temp_dir().join(LICENSES_FILE);
    if let Err(e) = std::fs::write(&path, fonts::licenses_text()) {
        return error_box(&format!("Could not write {}:\n{e}", path.display()));
    }
    let spawned =
        std::env::current_exe().and_then(|exe| std::process::Command::new(exe).arg(&path).spawn());
    if let Err(e) = spawned {
        error_box(&format!("Could not open {}:\n{e}", path.display()));
    }
}

unsafe fn about() {
    info_box(&format!(
        "{APP_NAME} {}\nA small, fast plain text editor.\n\n{} fonts bundled; see Help > Font Licenses.",
        env!("CARGO_PKG_VERSION"),
        fonts::BUNDLED.len(),
    ));
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
        Err(why) => {
            return error_box(&format!(
                "z++ will not open {} because {why}.",
                path.display()
            ))
        }
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
    let choice = MessageBoxW(
        app(|a| a.main.get()),
        text.as_ptr(),
        caption.as_ptr(),
        MB_YESNOCANCEL | MB_ICONWARNING,
    );
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
            let n = name.len().min(PATH_BUF_LEN - 1);
            buf[..n].copy_from_slice(&name[..n]);
        }
        initial_dir = p
            .parent()
            .map(|d| d.as_os_str().encode_wide().chain([0]).collect());
    }
    let filter: Vec<u16> = FILE_FILTER.encode_utf16().collect();
    let mut ofn: OPENFILENAMEW = zeroed();
    ofn.lStructSize = size_of::<OPENFILENAMEW>() as u32;
    ofn.hwndOwner = app(|a| a.main.get());
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

fn display_name() -> String {
    app(|a| {
        a.path
            .borrow()
            .as_ref()
            .and_then(|p| p.file_name())
            .map_or_else(
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

unsafe fn error_box(text: &str) {
    message_box(text, MB_OK | MB_ICONERROR);
}

unsafe fn info_box(text: &str) {
    message_box(text, MB_OK | MB_ICONINFORMATION);
}

unsafe fn message_box(text: &str, style: MESSAGEBOX_STYLE) {
    let text = wide(text);
    let caption = wide(APP_NAME);
    MessageBoxW(
        app(|a| a.main.get()),
        text.as_ptr(),
        caption.as_ptr(),
        style,
    );
}

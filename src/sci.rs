//! The Scintilla and Lexilla definitions z++ uses, from vendor/scintilla/include/Scintilla.h.
//! Both libraries are compiled into zpp.exe by build.rs.
#![allow(dead_code)]

use std::ffi::c_void;
use windows_sys::Win32::UI::Controls::NMHDR;

extern "C" {
    pub fn Scintilla_RegisterClasses(hinstance: *mut c_void) -> i32;
}

extern "system" {
    /// Lexilla: a lexer by name ("cpp", "python", ...), or null if there is none.
    pub fn CreateLexer(name: *const u8) -> *mut c_void;
}

/// Sent by the patched Scintilla (vendor/PATCHES.md) when files are dropped on the text.
/// wParam is the HDROP; Scintilla frees it, so the receiver must not call DragFinish.
pub const ZPP_WM_FILES_DROPPED: u32 = windows_sys::Win32::UI::WindowsAndMessaging::WM_APP + 7;

pub const SCI_ADDTEXT: u32 = 2001;
pub const SCI_CLEARDOCUMENTSTYLE: u32 = 2005;
pub const SCI_GETLENGTH: u32 = 2006;
pub const SCI_GETCURRENTPOS: u32 = 2008;
pub const SCI_GETANCHOR: u32 = 2009;
pub const SCI_REDO: u32 = 2011;
pub const SCI_SELECTALL: u32 = 2013;
pub const SCI_SETSAVEPOINT: u32 = 2014;
pub const SCI_GOTOPOS: u32 = 2025;
pub const SCI_CONVERTEOLS: u32 = 2029;
pub const SCI_SETEOLMODE: u32 = 2031;
pub const SCI_SETTABWIDTH: u32 = 2036;
pub const SCI_SETCODEPAGE: u32 = 2037;
pub const SCI_STYLECLEARALL: u32 = 2050;
pub const SCI_STYLESETFORE: u32 = 2051;
pub const SCI_STYLESETBACK: u32 = 2052;
pub const SCI_STYLESETBOLD: u32 = 2053;
pub const SCI_STYLESETITALIC: u32 = 2054;
pub const SCI_STYLESETFONT: u32 = 2056;
pub const SCI_STYLESETSIZEFRACTIONAL: u32 = 2061;
pub const SCI_BEGINUNDOACTION: u32 = 2078;
pub const SCI_ENDUNDOACTION: u32 = 2079;
pub const SCI_SETWHITESPACEFORE: u32 = 2084;
pub const SCI_SETCARETLINEVISIBLE: u32 = 2096;
pub const SCI_GETLINEENDPOSITION: u32 = 2136;
pub const SCI_GETSELECTIONSTART: u32 = 2143;
pub const SCI_GETSELECTIONEND: u32 = 2145;
pub const SCI_GETFIRSTVISIBLELINE: u32 = 2152;
pub const SCI_GETLINECOUNT: u32 = 2154;
pub const SCI_SETMARGINLEFT: u32 = 2155;
pub const SCI_SETMARGINRIGHT: u32 = 2157;
pub const SCI_GETMODIFY: u32 = 2159;
pub const SCI_SETSEL: u32 = 2160;
pub const SCI_GETSELTEXT: u32 = 2161;
pub const SCI_LINEFROMPOSITION: u32 = 2166;
pub const SCI_POSITIONFROMLINE: u32 = 2167;
pub const SCI_SCROLLCARET: u32 = 2169;
pub const SCI_REPLACESEL: u32 = 2170;
pub const SCI_EMPTYUNDOBUFFER: u32 = 2175;
pub const SCI_UNDO: u32 = 2176;
pub const SCI_CUT: u32 = 2177;
pub const SCI_COPY: u32 = 2178;
pub const SCI_PASTE: u32 = 2179;
pub const SCI_SETTEXT: u32 = 2181;
pub const SCI_GETTEXT: u32 = 2182;
pub const SCI_GETTARGETSTART: u32 = 2191;
pub const SCI_GETTARGETEND: u32 = 2193;
pub const SCI_REPLACETARGET: u32 = 2194;
pub const SCI_REPLACETARGETRE: u32 = 2195;
pub const SCI_SEARCHINTARGET: u32 = 2197;
pub const SCI_SETSEARCHFLAGS: u32 = 2198;
pub const SCI_SETMARGINTYPEN: u32 = 2240;
pub const SCI_SETMARGINWIDTHN: u32 = 2242;
pub const SCI_SETMARGINBACKN: u32 = 2250;
pub const SCI_SETMARGINS: u32 = 2252;
pub const SCI_SETWRAPMODE: u32 = 2268;
pub const SCI_SETSCROLLWIDTH: u32 = 2274;
pub const SCI_TEXTWIDTH: u32 = 2276;
pub const SCI_SETMODEVENTMASK: u32 = 2359;
pub const SCI_POSITIONBEFORE: u32 = 2417;
pub const SCI_POSITIONAFTER: u32 = 2418;
pub const SCI_USEPOPUP: u32 = 2371;
pub const SCI_GETZOOM: u32 = 2374;
pub const SCI_SETSCROLLWIDTHTRACKING: u32 = 2516;
pub const SCI_SETFIRSTVISIBLELINE: u32 = 2613;
pub const SCI_COUNTCHARACTERS: u32 = 2633;
pub const SCI_SETTARGETRANGE: u32 = 2686;
pub const SCI_SETELEMENTCOLOUR: u32 = 2753;
pub const SCI_COLOURISE: u32 = 4003;
pub const SCI_SETPROPERTY: u32 = 4004;
pub const SCI_SETKEYWORDS: u32 = 4005;
pub const SCI_SETILEXER: u32 = 4033;

pub const SCI_SETSTATUS: u32 = 2382;
pub const SCI_GETSTATUS: u32 = 2383;
pub const SC_STATUS_OK: usize = 0;
pub const SC_STATUS_WARN_REGEX: isize = 1001;

pub const SC_CP_UTF8: usize = 65001;
pub const SC_EOL_CRLF: usize = 0;
pub const SC_EOL_LF: usize = 2;
pub const SC_WRAP_NONE: usize = 0;
pub const SC_WRAP_WORD: usize = 1;
pub const SC_MARGIN_NUMBER: isize = 1;
pub const SC_POPUP_TEXT: usize = 2;
pub const SC_MOD_INSERTTEXT: usize = 0x1;
pub const SC_MOD_DELETETEXT: usize = 0x2;

pub const SCFIND_WHOLEWORD: usize = 0x2;
pub const SCFIND_MATCHCASE: usize = 0x4;
pub const SCFIND_REGEXP: usize = 0x0020_0000;
pub const SCFIND_CXX11REGEX: usize = 0x0080_0000;

pub const STYLE_DEFAULT: usize = 32;
pub const STYLE_LINENUMBER: usize = 33;
/// Styles 0..STYLE_DEFAULT belong to the lexer.
pub const LEXER_STYLES: usize = 32;

pub const SC_ELEMENT_SELECTION_TEXT: usize = 10;
pub const SC_ELEMENT_SELECTION_BACK: usize = 11;
pub const SC_ELEMENT_SELECTION_INACTIVE_BACK: usize = 17;
pub const SC_ELEMENT_CARET: usize = 40;
pub const SC_ELEMENT_CARET_LINE_BACK: usize = 50;
/// Element colors take an alpha byte; this one is opaque.
pub const ALPHA_OPAQUE: u32 = 0xFF00_0000;

pub const SCN_SAVEPOINTREACHED: u32 = 2002;
pub const SCN_SAVEPOINTLEFT: u32 = 2003;
pub const SCN_UPDATEUI: u32 = 2007;
pub const SCN_ZOOM: u32 = 2018;
pub const SC_UPDATE_CONTENT: i32 = 0x1;
pub const SC_UPDATE_SELECTION: i32 = 0x2;

#[repr(C)]
pub struct SCNotification {
    pub nmhdr: NMHDR,
    pub position: isize,
    pub ch: i32,
    pub modifiers: i32,
    pub modification_type: i32,
    pub text: *const u8,
    pub length: isize,
    pub lines_added: isize,
    pub message: i32,
    pub wparam: usize,
    pub lparam: isize,
    pub line: isize,
    pub fold_level_now: i32,
    pub fold_level_prev: i32,
    pub margin: i32,
    pub list_type: i32,
    pub x: i32,
    pub y: i32,
    pub token: i32,
    pub annotation_lines_added: isize,
    pub updated: i32,
    pub list_completion_method: i32,
    pub character_source: i32,
}

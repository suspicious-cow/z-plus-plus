//! The RichEdit definitions z++ uses, from Richedit.h. windows-sys 0.61 no longer ships them.
#![allow(non_snake_case, dead_code, clippy::upper_case_acronyms)]

const WM_USER: u32 = 0x0400;

pub const EM_EXLIMITTEXT: u32 = WM_USER + 53;
pub const EM_SETBKGNDCOLOR: u32 = WM_USER + 67;
pub const EM_SETCHARFORMAT: u32 = WM_USER + 68;
pub const EM_SETEVENTMASK: u32 = WM_USER + 69;
pub const EM_SETTARGETDEVICE: u32 = WM_USER + 72;
pub const EM_REDO: u32 = WM_USER + 84;
pub const EM_SETTEXTMODE: u32 = WM_USER + 89;
pub const EM_GETTEXTEX: u32 = WM_USER + 94;
pub const EM_GETTEXTLENGTHEX: u32 = WM_USER + 95;
pub const EM_SETLANGOPTIONS: u32 = WM_USER + 120;
pub const EM_GETLANGOPTIONS: u32 = WM_USER + 121;

pub const TM_PLAINTEXT: u32 = 1;
pub const TM_MULTILEVELUNDO: u32 = 8;
pub const TM_MULTICODEPAGE: u32 = 32;

pub const ENM_CHANGE: u32 = 0x0000_0001;

pub const IMF_AUTOFONT: u32 = 0x0002;
pub const IMF_DUALFONT: u32 = 0x0080;

pub const GTL_USECRLF: u32 = 1;
pub const GTL_PRECISE: u32 = 2;
pub const GTL_NUMCHARS: u32 = 8;
pub const GT_USECRLF: u32 = 1;

pub const SCF_DEFAULT: u32 = 0;
pub const SCF_ALL: u32 = 4;

pub const CFM_FACE: u32 = 0x2000_0000;
pub const CFM_COLOR: u32 = 0x4000_0000;
pub const CFM_SIZE: u32 = 0x8000_0000;

#[repr(C)]
pub struct CHARFORMATW {
    pub cbSize: u32,
    pub dwMask: u32,
    pub dwEffects: u32,
    pub yHeight: i32,
    pub yOffset: i32,
    pub crTextColor: u32,
    pub bCharSet: u8,
    pub bPitchAndFamily: u8,
    pub szFaceName: [u16; 32],
}

#[repr(C)]
pub struct GETTEXTEX {
    pub cb: u32,
    pub flags: u32,
    pub codepage: u32,
    pub lpDefaultChar: *const u8,
    pub lpUsedDefChar: *mut i32,
}

#[repr(C)]
pub struct GETTEXTLENGTHEX {
    pub flags: u32,
    pub codepage: u32,
}

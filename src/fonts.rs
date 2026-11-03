//! Fonts: a few common Windows monospace fonts, open-licensed fonts embedded in the exe,
//! and any installed font through the system font dialog.

use std::ffi::c_void;
use std::mem::zeroed;
use std::ptr::{null, null_mut};
use windows_sys::Win32::Foundation::LPARAM;
use windows_sys::Win32::Graphics::Gdi::{
    AddFontMemResourceEx, EnumFontFamiliesExW, GetDC, ReleaseDC, DEFAULT_CHARSET, LOGFONTW,
    TEXTMETRICW,
};

pub const DEFAULT_FACE: &str = "Consolas";
pub const DEFAULT_SIZE_TENTHS: u32 = 110;
pub const MIN_SIZE_TENTHS: u32 = 60;
pub const MAX_SIZE_TENTHS: u32 = 720;
pub const SIZE_STEP_TENTHS: u32 = 10;

/// Shown first in the Font menu when installed. They ship with Windows 11.
const WINDOWS_FONTS: &[&str] = &[
    "Consolas",
    "Cascadia Mono",
    "Cascadia Code",
    "Courier New",
    "Lucida Console",
];

pub struct Bundled {
    pub family: &'static str,
    data: &'static [u8],
    license: &'static str,
}

#[cfg(feature = "bundled-fonts")]
macro_rules! bundled {
    ($family:literal, $font:literal, $license:literal) => {
        Bundled {
            family: $family,
            data: include_bytes!(concat!("../fonts/", $font)),
            license: include_str!(concat!("../fonts/licenses/", $license)),
        }
    };
}

#[cfg(feature = "bundled-fonts")]
pub const BUNDLED: &[Bundled] = &[
    bundled!(
        "Anonymous Pro",
        "AnonymousPro-Regular.ttf",
        "AnonymousPro-OFL.txt"
    ),
    bundled!(
        "Courier Prime",
        "CourierPrime-Regular.ttf",
        "CourierPrime-OFL.txt"
    ),
    bundled!("Fira Mono", "FiraMono-Regular.ttf", "FiraMono-OFL.txt"),
    bundled!("Go Mono", "Go-Mono.ttf", "GoMono-LICENSE.txt"),
    bundled!("Hack", "Hack-Regular.ttf", "Hack-LICENSE.txt"),
    bundled!(
        "IBM Plex Mono",
        "IBMPlexMono-Regular.ttf",
        "IBMPlexMono-OFL.txt"
    ),
    bundled!(
        "JetBrains Mono",
        "JetBrainsMono-Regular.ttf",
        "JetBrainsMono-OFL.txt"
    ),
    bundled!("PT Mono", "PTM55FT.ttf", "PTMono-OFL.txt"),
    bundled!(
        "Source Code Pro",
        "SourceCodePro-Regular.ttf",
        "SourceCodePro-OFL.txt"
    ),
    bundled!("Space Mono", "SpaceMono-Regular.ttf", "SpaceMono-OFL.txt"),
    bundled!(
        "Ubuntu Mono",
        "UbuntuMono-Regular.ttf",
        "UbuntuMono-UFL.txt"
    ),
];

#[cfg(not(feature = "bundled-fonts"))]
pub const BUNDLED: &[Bundled] = &[];

/// Makes the embedded fonts usable by this process only. Nothing is installed.
pub unsafe fn register_bundled() {
    for font in BUNDLED {
        // Windows writes the font count through this pointer, so it must point at writable memory.
        let mut count = 0u32;
        AddFontMemResourceEx(
            font.data.as_ptr() as *const c_void,
            font.data.len() as u32,
            null(),
            std::ptr::addr_of_mut!(count),
        );
    }
}

/// The WINDOWS_FONTS that are installed on this machine.
pub fn windows_fonts() -> Vec<&'static str> {
    WINDOWS_FONTS
        .iter()
        .copied()
        .filter(|f| is_installed(f))
        .collect()
}

pub fn is_available(face: &str) -> bool {
    BUNDLED.iter().any(|b| b.family.eq_ignore_ascii_case(face)) || is_installed(face)
}

pub fn clamp_size(tenths: u32) -> u32 {
    tenths.clamp(MIN_SIZE_TENTHS, MAX_SIZE_TENTHS)
}

/// All bundled font licenses as one text, for Help > Font Licenses.
pub fn licenses_text() -> String {
    let mut out = String::from("Fonts bundled with z++ and their licenses.\r\n");
    for font in BUNDLED {
        out.push_str("\r\n==================== ");
        out.push_str(font.family);
        out.push_str(" ====================\r\n\r\n");
        out.push_str(font.license.trim_start_matches('\u{feff}'));
        out.push_str("\r\n");
    }
    out
}

/// Fonts added with AddFontMemResourceEx are not enumerable, so this only sees installed fonts.
pub fn is_installed(face: &str) -> bool {
    let mut lf: LOGFONTW = unsafe { zeroed() };
    lf.lfCharSet = DEFAULT_CHARSET;
    let units: Vec<u16> = face.encode_utf16().take(lf.lfFaceName.len() - 1).collect();
    lf.lfFaceName[..units.len()].copy_from_slice(&units);
    let mut is_found = false;
    unsafe {
        let hdc = GetDC(null_mut());
        EnumFontFamiliesExW(
            hdc,
            &lf,
            Some(on_font),
            &mut is_found as *mut bool as LPARAM,
            0,
        );
        ReleaseDC(null_mut(), hdc);
    }
    is_found
}

unsafe extern "system" fn on_font(
    _: *const LOGFONTW,
    _: *const TEXTMETRICW,
    _: u32,
    lparam: LPARAM,
) -> i32 {
    *(lparam as *mut bool) = true;
    0
}

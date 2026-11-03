//! Color themes. Colors are Win32 COLORREF values (0x00BBGGRR).

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct Colors {
    pub fg: u32,
    pub bg: u32,
}

pub struct Preset {
    pub name: &'static str,
    pub colors: Colors,
}

pub const fn rgb(r: u8, g: u8, b: u8) -> u32 {
    r as u32 | (g as u32) << 8 | (b as u32) << 16
}

const fn preset(name: &'static str, fg: u32, bg: u32) -> Preset {
    Preset {
        name,
        colors: Colors { fg, bg },
    }
}

pub const LIGHT: &str = "Light";
pub const DARK: &str = "Dark";
pub const CUSTOM: &str = "Custom";

/// A web-style 0xRRGGBB value as a COLORREF.
pub const fn hex(v: u32) -> u32 {
    rgb((v >> 16) as u8, (v >> 8) as u8, v as u8)
}

/// Light themes first, then dark ones; Light and Dark lead each group, the rest are alphabetical.
/// Themes marked "Notepad++" use the default text and background colors from the theme files
/// that ship with Notepad++ (scripts/import_npp_themes.py reads them).
pub const PRESETS: &[Preset] = &[
    preset(LIGHT, hex(0x000000), hex(0xFFFFFF)),
    preset("Gruvbox Light", hex(0x3C3836), hex(0xFBF1C7)),
    preset("Hello Kitty", hex(0x000000), hex(0xFFB0FF)), // Notepad++
    preset("Khaki", hex(0x5F5F00), hex(0xD7D7AF)),       // Notepad++
    preset("Navajo", hex(0x000000), hex(0xBA9C80)),      // Notepad++
    preset("Paper", hex(0x5B4636), hex(0xF4ECD8)),
    preset("Solarized Light", hex(0x657B83), hex(0xFDF6E3)), // Notepad++
    preset(DARK, hex(0xD4D4D4), hex(0x1E1E1E)),
    preset("Bespin", hex(0xBDAE9D), hex(0x2A211C)), // Notepad++
    preset("Black Board", hex(0xF8F8F8), hex(0x0C1021)), // Notepad++
    preset("Choco", hex(0xC3BE98), hex(0x1A0F0B)),  // Notepad++
    preset("DansLeRuSH Dark", hex(0xC7C7C7), hex(0x2E2E2E)), // Notepad++
    preset("Dark Mode Default", hex(0xDCDCCC), hex(0x3F3F3F)), // Notepad++
    preset("Deep Black", hex(0xFFFFFF), hex(0x000000)), // Notepad++
    preset("Dracula", hex(0xF8F8F2), hex(0x282A36)),
    preset("Green Screen", hex(0x33FF66), hex(0x0C0C0C)),
    preset("Gruvbox Dark", hex(0xEBDBB2), hex(0x282828)),
    preset("High Contrast", hex(0xFFFFFF), hex(0x000000)),
    preset("Hot Fudge Sundae", hex(0xB7975D), hex(0x2B0F01)), // Notepad++
    preset("Mono Industrial", hex(0xFFFFFF), hex(0x222C28)),  // Notepad++
    preset("Monokai", hex(0xF8F8F2), hex(0x272822)),          // Notepad++
    preset("Mossy Lawn", hex(0xF2C476), hex(0x58693D)),       // Notepad++
    preset("Nord", hex(0xD8DEE9), hex(0x2E3440)),
    preset("Obsidian", hex(0xE0E2E4), hex(0x293134)), // Notepad++
    preset("One Dark", hex(0xABB2BF), hex(0x282C34)),
    preset("Plastic Code Wrap", hex(0xF8F8F8), hex(0x0B161D)), // Notepad++
    preset("Ruby Blue", hex(0xFFFFFF), hex(0x112435)),         // Notepad++
    preset("Solarized Dark", hex(0x839496), hex(0x002B36)),    // Notepad++
    preset("Twilight", hex(0xF8F8F8), hex(0x141414)),          // Notepad++
    preset("Vibrant Ink", hex(0xFFFFFF), hex(0x000000)),       // Notepad++
    preset("Vim Dark Blue", hex(0xFFFFBF), hex(0x000040)),     // Notepad++
    preset("Zenburn", hex(0xDCDCCC), hex(0x3F3F3F)),           // Notepad++
];

/// Index of the first dark theme; PRESETS before it are light.
pub fn first_dark() -> usize {
    PRESETS
        .iter()
        .position(|p| is_dark(p.colors.bg))
        .unwrap_or(PRESETS.len())
}

/// Index into PRESETS, or PRESETS.len() for the custom colors.
pub fn index_of(name: &str) -> Option<usize> {
    if name.eq_ignore_ascii_case(CUSTOM) {
        return Some(PRESETS.len());
    }
    PRESETS
        .iter()
        .position(|p| p.name.eq_ignore_ascii_case(name))
}

pub fn name_of(index: usize) -> &'static str {
    PRESETS.get(index).map_or(CUSTOM, |p| p.name)
}

pub fn colors_of(index: usize, custom: Colors) -> Colors {
    PRESETS.get(index).map_or(custom, |p| p.colors)
}

/// Dark backgrounds get dark window chrome (title bar, menus, scroll bars).
pub fn is_dark(color: u32) -> bool {
    let (r, g, b) = channels(color);
    299 * r + 587 * g + 114 * b < 128_000
}

/// Blends `a` toward `b` by `percent`.
pub fn mix(a: u32, b: u32, percent: u32) -> u32 {
    let (ar, ag, ab) = channels(a);
    let (br, bg, bb) = channels(b);
    let blend = |x: u32, y: u32| ((x * (100 - percent) + y * percent) / 100) as u8;
    rgb(blend(ar, br), blend(ag, bg), blend(ab, bb))
}

fn channels(c: u32) -> (u32, u32, u32) {
    (c & 0xFF, (c >> 8) & 0xFF, (c >> 16) & 0xFF)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn darkness() {
        assert!(!is_dark(rgb(0xFF, 0xFF, 0xFF)));
        assert!(is_dark(rgb(0x1E, 0x1E, 0x1E)));
        assert!(is_dark(rgb(0x00, 0x2B, 0x36)));
        assert!(!is_dark(rgb(0xFD, 0xF6, 0xE3)));
    }

    #[test]
    fn names_round_trip() {
        for (i, p) in PRESETS.iter().enumerate() {
            assert_eq!(index_of(p.name), Some(i));
        }
        assert_eq!(index_of("custom"), Some(PRESETS.len()));
        assert_eq!(name_of(PRESETS.len()), CUSTOM);
    }

    #[test]
    fn light_themes_come_before_dark_ones() {
        let split = first_dark();
        assert_eq!(PRESETS[0].name, LIGHT);
        assert_eq!(PRESETS[split].name, DARK);
        assert!(PRESETS[..split].iter().all(|p| !is_dark(p.colors.bg)));
        assert!(PRESETS[split..].iter().all(|p| is_dark(p.colors.bg)));
    }

    #[test]
    fn names_are_unique_and_sorted_within_groups() {
        let split = first_dark();
        for group in [&PRESETS[1..split], &PRESETS[split + 1..]] {
            let names: Vec<_> = group.iter().map(|p| p.name.to_lowercase()).collect();
            let mut sorted = names.clone();
            sorted.sort();
            sorted.dedup();
            assert_eq!(names, sorted);
        }
    }

    #[test]
    fn hex_is_rgb_order() {
        assert_eq!(hex(0x112233), rgb(0x11, 0x22, 0x33));
    }
}

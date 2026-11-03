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
    Preset { name, colors: Colors { fg, bg } }
}

pub const LIGHT: &str = "Light";
pub const DARK: &str = "Dark";
pub const CUSTOM: &str = "Custom";

pub const PRESETS: &[Preset] = &[
    preset(LIGHT, rgb(0x00, 0x00, 0x00), rgb(0xFF, 0xFF, 0xFF)),
    preset(DARK, rgb(0xD4, 0xD4, 0xD4), rgb(0x1E, 0x1E, 0x1E)),
    preset("Paper", rgb(0x5B, 0x46, 0x36), rgb(0xF4, 0xEC, 0xD8)),
    preset("Solarized Light", rgb(0x65, 0x7B, 0x83), rgb(0xFD, 0xF6, 0xE3)),
    preset("Solarized Dark", rgb(0x83, 0x94, 0x96), rgb(0x00, 0x2B, 0x36)),
    preset("Gruvbox Light", rgb(0x3C, 0x38, 0x36), rgb(0xFB, 0xF1, 0xC7)),
    preset("Gruvbox Dark", rgb(0xEB, 0xDB, 0xB2), rgb(0x28, 0x28, 0x28)),
    preset("Nord", rgb(0xD8, 0xDE, 0xE9), rgb(0x2E, 0x34, 0x40)),
    preset("Dracula", rgb(0xF8, 0xF8, 0xF2), rgb(0x28, 0x2A, 0x36)),
    preset("One Dark", rgb(0xAB, 0xB2, 0xBF), rgb(0x28, 0x2C, 0x34)),
    preset("Monokai", rgb(0xF8, 0xF8, 0xF2), rgb(0x27, 0x28, 0x22)),
    preset("Green Screen", rgb(0x33, 0xFF, 0x66), rgb(0x0C, 0x0C, 0x0C)),
    preset("High Contrast", rgb(0xFF, 0xFF, 0xFF), rgb(0x00, 0x00, 0x00)),
];

/// Index into PRESETS, or PRESETS.len() for the custom colors.
pub fn index_of(name: &str) -> Option<usize> {
    if name.eq_ignore_ascii_case(CUSTOM) {
        return Some(PRESETS.len());
    }
    PRESETS.iter().position(|p| p.name.eq_ignore_ascii_case(name))
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
    fn presets_are_classified_as_named() {
        for p in PRESETS {
            let looks_light = p.name.contains("Light") || p.name == "Paper";
            if looks_light {
                assert!(!is_dark(p.colors.bg), "{}", p.name);
            }
        }
    }
}

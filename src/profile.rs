//! `ZPP_PROFILE=<name>` gives z++ its own settings key and its own single-instance window, so a
//! test run or a second setup never reads, writes, or hands files to the everyday z++.

use std::sync::OnceLock;

const ENV_VAR: &str = "ZPP_PROFILE";
/// With a profile set, ZPP_TEST=1 runs z++ for automated checks: see is_test.
const TEST_VAR: &str = "ZPP_TEST";
const BASE_KEY: &str = "Software\\z++";
const BASE_CLASS: &str = "ZppMainWindow";
const MAX_NAME: usize = 40;

fn name() -> Option<&'static str> {
    static NAME: OnceLock<Option<String>> = OnceLock::new();
    NAME.get_or_init(|| std::env::var(ENV_VAR).ok().filter(|n| is_valid(n)))
        .as_deref()
}

/// Letters, digits, `-` and `_` only, so the name is safe in a registry path and a class name.
fn is_valid(name: &str) -> bool {
    !name.is_empty()
        && name.len() <= MAX_NAME
        && name
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
}

fn key_for(name: Option<&str>) -> String {
    match name {
        Some(n) => format!("{BASE_KEY}\\Profiles\\{n}"),
        None => BASE_KEY.to_string(),
    }
}

fn class_for(name: Option<&str>) -> String {
    match name {
        Some(n) => format!("{BASE_CLASS}.{n}"),
        None => BASE_CLASS.to_string(),
    }
}

/// GUI checks: the window opens off-screen without taking focus or a taskbar button, and counts
/// as in front whenever it is not minimized, so a test never pops up on the user's screen.
pub fn is_test() -> bool {
    static IS_TEST: OnceLock<bool> = OnceLock::new();
    *IS_TEST.get_or_init(|| name().is_some() && std::env::var(TEST_VAR).is_ok_and(|v| v == "1"))
}

/// HKCU subkey holding this profile's settings, NUL-terminated.
pub fn prefs_key() -> *const u16 {
    static KEY: OnceLock<Vec<u16>> = OnceLock::new();
    KEY.get_or_init(|| crate::wide(&key_for(name()))).as_ptr()
}

/// Window class of this profile's main window, NUL-terminated.
pub fn class_name() -> *const u16 {
    static CLASS: OnceLock<Vec<u16>> = OnceLock::new();
    CLASS
        .get_or_init(|| crate::wide(&class_for(name())))
        .as_ptr()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn names_map_to_their_own_key_and_class() {
        assert_eq!(key_for(None), "Software\\z++");
        assert_eq!(class_for(None), "ZppMainWindow");
        assert_eq!(key_for(Some("test-1")), "Software\\z++\\Profiles\\test-1");
        assert_eq!(class_for(Some("test-1")), "ZppMainWindow.test-1");
    }

    #[test]
    fn rejects_names_that_could_escape_the_key() {
        assert!(is_valid("gui_test-2"));
        for bad in ["", "a\\b", "..", "a b", "x/y", &"n".repeat(MAX_NAME + 1)] {
            assert!(!is_valid(bad), "{bad:?}");
        }
    }
}

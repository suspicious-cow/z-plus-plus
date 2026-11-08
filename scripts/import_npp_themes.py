"""Reads the default text and background color from each theme that ships with Notepad++.

This script takes each theme's text and background color from its "Default Style" (in Global
Styles) and prints them as Rust preset lines for src/themes.rs. Code colors come from
scripts/import_npp_syntax.py. Only the two color values are used; no theme file is copied.

Run: uv run --with defusedxml python scripts/import_npp_themes.py
"""

import json
import sys
import urllib.request

from defusedxml import ElementTree

REPO_API = "https://api.github.com/repos/notepad-plus-plus/notepad-plus-plus/contents/PowerEditor/installer/themes"
RAW_BASE = "https://raw.githubusercontent.com/notepad-plus-plus/notepad-plus-plus/master/PowerEditor/installer/themes/"

# Display names for files whose names are not already readable.
DISPLAY_NAMES = {
    "DansLeRuSH-Dark": "DansLeRuSH Dark",
    "DarkModeDefault": "Dark Mode Default",
    "HotFudgeSundae": "Hot Fudge Sundae",
    "MossyLawn": "Mossy Lawn",
    "Solarized-light": "Solarized Light",
    "Solarized": "Solarized Dark",
    "khaki": "Khaki",
    "vim Dark Blue": "Vim Dark Blue",
}


def fetch(url: str) -> bytes:
    with urllib.request.urlopen(url, timeout=30) as resp:
        return resp.read()


def default_style(xml: bytes) -> tuple[str, str]:
    root = ElementTree.fromstring(xml)
    for style in root.iter("WidgetStyle"):
        if style.get("name") == "Default Style":
            return style.get("fgColor"), style.get("bgColor")
    raise ValueError("no Default Style")


def main() -> None:
    files = [f["name"] for f in json.loads(fetch(REPO_API)) if f["name"].endswith(".xml")]
    for name in sorted(files, key=str.lower):
        stem = name[: -len(".xml")]
        fg, bg = default_style(fetch(RAW_BASE + urllib.request.quote(name)))
        label = DISPLAY_NAMES.get(stem, stem)
        print(f'    preset("{label}", hex(0x{fg.upper()}), hex(0x{bg.upper()})),', file=sys.stdout)


if __name__ == "__main__":
    main()

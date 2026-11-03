# <img src="assets/zpp-256.png" alt="" width="48" align="top"> z++

A small, fast plain text editor for Windows, written in Rust. It runs natively on ARM64 and x64.

![z++ with the Dark theme](docs/screenshots/dark.png)

z++ is one exe. There is no installer and no runtime to install. It talks to the Win32 API directly and edits text with the Windows RichEdit control, so it opens fast and stays small.

The project is early. It edits plain text well, but it is not a code editor: there is no syntax highlighting, no tabs and no find and replace yet. See [CHANGELOG.md](CHANGELOG.md) for what has landed.

## Features

- New, Open, Save and Save As. Pass a file on the command line to open it: `zpp.exe notes.txt`.
- Files keep their encoding (UTF-8, UTF-8 with BOM, UTF-16 LE or ANSI) and their LF line endings. z++ refuses to open a file it could not save back unchanged, such as a binary file, instead of damaging it.
- 13 color themes, plus your own text and background colors. Dark themes also darken the title bar, menus and scroll bars.
- 11 open-licensed monospace fonts built into the exe, the usual Windows monospace fonts, and any font installed on the machine.
- A toolbar under the menu bar, grouped like the Home tab in Office: file commands, clipboard, font, size, colors, theme and word wrap. View > Toolbar hides it.
- Settings are saved per user under `HKCU\Software\z++`.

| Solarized Light with Fira Mono | Nord with JetBrains Mono |
| --- | --- |
| ![Solarized Light theme](docs/screenshots/solarized-light.png) | ![Nord theme](docs/screenshots/nord.png) |

## Download

Get the latest build from the [Releases page](https://github.com/suspicious-cow/z-plus-plus/releases/latest). Pick `arm64` for Windows on Arm or `x64` for Intel and AMD PCs, unzip it, and run `zpp.exe`. You can run `zpp.exe` straight from the folder, or install it for your user account (no admin rights):

```
pwsh -File install.ps1
```

The installer copies z++ to `%LOCALAPPDATA%\Programs\z++` and adds a Start menu shortcut, `zpp` in Win+R, z++ in the "Open with" list for common text files, "Edit with z++" on the right-click menu, and an entry in Settings > Apps for removing it. Windows 11 doesn't let apps make themselves the default, so to open `.txt` files in z++ by default, right-click one, choose Open with > z++ and pick "Always". From a source checkout, run `pwsh -File scripts\install.ps1` after building.

The exe is not code-signed yet, so Windows SmartScreen may warn the first time you run it.

## Keyboard shortcuts

| Action | Keys |
| --- | --- |
| New, Open, Save | Ctrl+N, Ctrl+O, Ctrl+S |
| Save As | Ctrl+Shift+S |
| Undo, Redo | Ctrl+Z, Ctrl+Y |
| Cut, Copy, Paste, Select All | Ctrl+X, Ctrl+C, Ctrl+V, Ctrl+A |
| Bigger, smaller, reset font size | Ctrl+=, Ctrl+-, Ctrl+0 |
| Word wrap | Alt+Z |
| Switch between your last light and dark theme | Ctrl+Shift+D |

## Building

You need:

- Rust stable with the MSVC toolchain, from [rustup](https://rustup.rs).
- Visual Studio 2022 Build Tools with the C++ build tools for each target you build (ARM64 and/or x64) and a Windows 11 SDK.

Then:

```
rustup target add aarch64-pc-windows-msvc x86_64-pc-windows-msvc
cargo build --release --target aarch64-pc-windows-msvc
cargo build --release --target x86_64-pc-windows-msvc
```

The exe lands in `target\<target>\release\zpp.exe`. The C runtime is linked statically, so the exe runs on a clean Windows 11 machine.

To leave the bundled fonts out, build with `--no-default-features`. On the development machine (Windows 11 ARM64, measured 2026-11-03), the ARM64 release exe was 2.4 MB with the fonts and 361 KB without them.

Run the tests with `cargo test`.

## Code layout

| File | What it does |
| --- | --- |
| [src/window.rs](src/window.rs) | Main window, commands, open and save |
| [src/toolbar.rs](src/toolbar.rs) | The toolbar, drawn in the current theme's colors |
| [src/menu.rs](src/menu.rs) | Menu bar, command IDs, keyboard shortcuts |
| [src/editor.rs](src/editor.rs) | The RichEdit control in plain-text mode |
| [src/file.rs](src/file.rs) | Reading and writing encodings and line endings |
| [src/themes.rs](src/themes.rs) | Theme presets and color math |
| [src/theme.rs](src/theme.rs) | Applying colors and fonts to the window, dark menu bar painting |
| [src/fonts.rs](src/fonts.rs) | Bundled fonts and the font list |
| [src/prefs.rs](src/prefs.rs) | Settings in the registry |

Dark menus rely on two undocumented uxtheme.dll functions, the same approach Notepad++ uses. If a Windows update removes them, the menus fall back to light and everything else keeps working.

## Bundled fonts

Each font is the Regular weight, embedded in the exe and loaded for z++ only. Nothing is installed on the machine. The license for each one is in [fonts/licenses](fonts/licenses).

| Font | License | Source file |
| --- | --- | --- |
| Anonymous Pro | [SIL OFL 1.1](fonts/licenses/AnonymousPro-OFL.txt) | [google/fonts](https://github.com/google/fonts/raw/main/ofl/anonymouspro/AnonymousPro-Regular.ttf) |
| Courier Prime | [SIL OFL 1.1](fonts/licenses/CourierPrime-OFL.txt) | [google/fonts](https://github.com/google/fonts/raw/main/ofl/courierprime/CourierPrime-Regular.ttf) |
| Fira Mono | [SIL OFL 1.1](fonts/licenses/FiraMono-OFL.txt) | [google/fonts](https://github.com/google/fonts/raw/main/ofl/firamono/FiraMono-Regular.ttf) |
| Go Mono | [BSD 3-Clause](fonts/licenses/GoMono-LICENSE.txt) | [golang/image](https://github.com/golang/image/raw/master/font/gofont/ttfs/Go-Mono.ttf) |
| Hack | [MIT and Bitstream Vera](fonts/licenses/Hack-LICENSE.txt) | [source-foundry/Hack](https://github.com/source-foundry/Hack/raw/master/build/ttf/Hack-Regular.ttf) |
| IBM Plex Mono | [SIL OFL 1.1](fonts/licenses/IBMPlexMono-OFL.txt) | [google/fonts](https://github.com/google/fonts/raw/main/ofl/ibmplexmono/IBMPlexMono-Regular.ttf) |
| JetBrains Mono | [SIL OFL 1.1](fonts/licenses/JetBrainsMono-OFL.txt) | [JetBrains/JetBrainsMono](https://github.com/JetBrains/JetBrainsMono/raw/master/fonts/ttf/JetBrainsMono-Regular.ttf) |
| PT Mono | [SIL OFL 1.1](fonts/licenses/PTMono-OFL.txt) | [google/fonts](https://github.com/google/fonts/raw/main/ofl/ptmono/PTM55FT.ttf) |
| Source Code Pro | [SIL OFL 1.1](fonts/licenses/SourceCodePro-OFL.txt) | [adobe-fonts/source-code-pro](https://github.com/adobe-fonts/source-code-pro/raw/release/TTF/SourceCodePro-Regular.ttf) |
| Space Mono | [SIL OFL 1.1](fonts/licenses/SpaceMono-OFL.txt) | [google/fonts](https://github.com/google/fonts/raw/main/ofl/spacemono/SpaceMono-Regular.ttf) |
| Ubuntu Mono | [Ubuntu Font Licence 1.0](fonts/licenses/UbuntuMono-UFL.txt) | [google/fonts](https://github.com/google/fonts/raw/main/ufl/ubuntumono/UbuntuMono-Regular.ttf) |

In the app, Help > Font Licenses shows all of these license texts.

## Contributing

Bug reports and ideas are welcome in [Issues](https://github.com/suspicious-cow/z-plus-plus/issues). See [CONTRIBUTING.md](CONTRIBUTING.md) before sending a pull request.

## License

The z++ source code is licensed under the [Apache License 2.0](LICENSE). The bundled fonts keep their own licenses, listed above. See [NOTICE](NOTICE).

# <img src="assets/zpp-256.png" alt="" width="48" align="top"> z++

A small, fast plain text editor for Windows, written in Rust. It runs natively on ARM64 and x64.

![z++ with the Dark theme](docs/screenshots/dark.png)

z++ is one exe. There is no installer and no runtime to install. It talks to the Win32 API directly and edits text with [Scintilla](https://www.scintilla.org), the editing component Notepad++ uses, compiled into the exe. It opens fast and stays small.

The project is early. See [CHANGELOG.md](CHANGELOG.md) for what has landed.

## Features

- Tabs: each open file gets a tab with its own undo history. A dot on a tab means unsaved changes. Opening a file that is already open switches to its tab, and a file opened from Explorer while z++ is running opens as a new tab in that window (`zpp.exe --new-window` opens a second window instead).
- New, Open (several files at once), Save, Save As, Save All. Drag files onto the window to open them.
- z++ reopens last time's files and window size on start, and keeps the last 10 files under File > Recent Files.
- Saving writes to a temporary file first and swaps it in, so a crash or full disk during a save leaves the old file intact.
- If another program changes an open file, z++ reloads it, changing only the lines that differ and marking them in the margin. If you have unsaved edits in that tab, a bar above the text asks instead of a dialog. See [Command line, git and coding agents](#command-line-git-and-coding-agents).
- Syntax highlighting for 24 languages, picked from the file name: Batch, C, C#, C++, CSS, Diff, Go, HTML, INI, Java, JavaScript, JSON, Lua, Makefile, Markdown, PowerShell, Python, Rust, Shell, SQL, TOML, TypeScript, XML and YAML. The Language menu switches a tab to another language or to plain text. Code colors follow the theme.
- Find in Files (Ctrl+Shift+F): search a folder and its subfolders, with a file filter such as `*.rs;*.toml`. Version control, dependency and build folders (`.git`, `node_modules`, `target`, `.venv` and similar) are skipped, and so are binary files. The hits fill a panel under the text; double-click one to open the file at that spot. Regular expressions here use [Rust's regex syntax](https://docs.rs/regex/latest/regex/#syntax) (no backreferences or lookaround), which keeps searches fast on big folders.
- Line numbers (View > Line Numbers).
- Find, Find Next and Previous, Replace and Replace All (one undo step), and Go to Line. Find and Replace can match case, whole words, or a regular expression (ECMAScript syntax), and a replacement can use the groups the expression captured as `\1` to `\9`.
- A status bar with line and column, the selection size, zoom, line endings and encoding. Click the encoding or line endings to change them; Format > Encoding and Format > Line Endings do the same.
- Files keep their encoding (UTF-8, UTF-8 with BOM, UTF-16 LE or ANSI) and their LF line endings unless you change them. z++ refuses to open a file it could not save back unchanged, such as a binary file, instead of damaging it.
- 32 color themes, including all 22 that ship with Notepad++, plus your own text and background colors. Point at a theme in View > Theme or in the toolbar's theme list and the whole window shows it right away; move off it or press Escape to go back. Dark themes also darken the title bar, menus and scroll bars.
- 11 open-licensed monospace fonts built into the exe, the usual Windows monospace fonts, and any font installed on the machine.
- A toolbar under the menu bar, grouped like the Home tab in Office: file commands, clipboard, find, font, size, colors, theme and word wrap. View > Toolbar and View > Status Bar hide those bars.
- Settings are saved per user under `HKCU\Software\z++`.

| Solarized Light with Fira Mono | Nord with JetBrains Mono |
| --- | --- |
| ![Solarized Light theme](docs/screenshots/solarized-light.png) | ![Nord theme](docs/screenshots/nord.png) |

## Download

Get the latest build from the [Releases page](https://github.com/suspicious-cow/z-plus-plus/releases/latest). Pick `arm64` for Windows on Arm or `x64` for Intel and AMD PCs, unzip it, and run `zpp.exe`. You can run `zpp.exe` straight from the folder, or install it for your user account (no admin rights):

```
pwsh -File install.ps1
```

The installer copies z++ to `%LOCALAPPDATA%\Programs\z++` and adds a Start menu shortcut, `zpp` in Win+R, "Edit with z++" on the right-click menu, and an entry in Settings > Apps for removing it. It registers z++ for the 91 file types that Windows Notepad and Notepad++ open, and makes z++ the default for any of them that has no default app yet. Windows 11 doesn't let an app take over a type that already has a default, such as `.txt` from Notepad, so switch those in Settings > Apps > Default apps: search for z++ there and pick it for each type. When another app can also open a type, Windows asks once the first time you open one; choose z++ and "Always". From a source checkout, run `pwsh -File scripts\install.ps1` after building.

The exe is not code-signed yet, so Windows SmartScreen may warn the first time you run it.

## Command line, git and coding agents

```
zpp file.txt                 open a file (in the z++ already running, if there is one)
zpp src\main.rs:42           open at line 42
zpp src\main.rs:42:7         open at line 42, character 7
zpp --wait file.txt          return only once the file's tab is closed
zpp --new-window file.txt    open in a separate window
```

`file:line` and `file:line:col` are the form compilers, grep and coding agents print, so a location can be pasted as it is.

`--wait` (or `-w`) makes z++ usable as the editor for tools that wait on one. For git, run this once in PowerShell:

```powershell
git config --global core.editor "'$($env:LOCALAPPDATA -replace '\\','/')/Programs/z++/zpp.exe' --wait"
```

Other tools read the `EDITOR` or `VISUAL` environment variable. In Claude Code, Ctrl+G opens the prompt in an external editor; its docs do not say which variable it reads, so set both:

```powershell
[Environment]::SetEnvironmentVariable('EDITOR', "`"$env:LOCALAPPDATA\Programs\z++\zpp.exe`" --wait", 'User')
[Environment]::SetEnvironmentVariable('VISUAL', "`"$env:LOCALAPPDATA\Programs\z++\zpp.exe`" --wait", 'User')
```

z++ also keeps up while an agent edits your files:

- z++ checks the open file about once a second, even in the background. An unchanged tab reloads by itself, keeping your place.
- A reload changes only the lines that differ and marks them in a thin margin left of the text. Your own unsaved edits get a mark there too. Undo goes back to the text before the reload.
- If you have unsaved edits when the file changes on disk, a bar above the text offers Reload or Keep mine. Nothing pops up and nothing waits on you.
- z++ never holds files open or locked, and it saves through a temporary file, so a save from z++ and a write from another program don't collide halfway.

## Keyboard shortcuts

| Action | Keys |
| --- | --- |
| New tab, Open, Save | Ctrl+N, Ctrl+O, Ctrl+S |
| Save As | Ctrl+Shift+S |
| Close tab | Ctrl+W, or middle-click the tab |
| Next, previous tab | Ctrl+Tab, Ctrl+Shift+Tab (or Ctrl+PgDn, Ctrl+PgUp) |
| Undo, Redo | Ctrl+Z, Ctrl+Y |
| Cut, Copy, Paste, Select All | Ctrl+X, Ctrl+C, Ctrl+V, Ctrl+A |
| Find, Replace | Ctrl+F, Ctrl+H |
| Find next, previous | F3, Shift+F3 |
| Find in Files | Ctrl+Shift+F |
| Go to line | Ctrl+G |
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

To leave the bundled fonts out, build with `--no-default-features`. On the development machine (Windows 11 ARM64, measured 2026-11-07), the ARM64 release exe was 4.5 MB with the fonts and 2.6 MB without them. Scintilla, the Lexilla lexers and the regex engine for Find in Files account for most of the second number. Only the lexers listed in [native/lexers.cxx](native/lexers.cxx) are linked in.

Run the tests with `cargo test`.

## Code layout

| File | What it does |
| --- | --- |
| [src/window.rs](src/window.rs) | Main window, layout, commands, themes and fonts |
| [src/docs.rs](src/docs.rs) | Open files: tabs, open, save, close, reload, recent files, session |
| [src/toolbar.rs](src/toolbar.rs) | The toolbar, drawn in the current theme's colors |
| [src/tabs.rs](src/tabs.rs) | The tab strip |
| [src/statusbar.rs](src/statusbar.rs) | The status bar |
| [src/find.rs](src/find.rs) | Find and Replace |
| [src/findfiles.rs](src/findfiles.rs) | The Find in Files dialog |
| [src/search.rs](src/search.rs) | Find in Files: walking the folder and matching, on a background thread |
| [src/results.rs](src/results.rs) | The Find in Files results panel |
| [src/dialog.rs](src/dialog.rs) | Dialog templates built in memory, and the Go to Line dialog |
| [src/gdi.rs](src/gdi.rs) | Drawing helpers shared by the toolbar, tabs and status bar |
| [src/menu.rs](src/menu.rs) | Menu bar, command IDs, keyboard shortcuts |
| [src/editor.rs](src/editor.rs) | The Scintilla edit control: text, selection, search |
| [src/sci.rs](src/sci.rs) | Scintilla message numbers and the notification struct |
| [src/notice.rs](src/notice.rs) | The bar above the text for "changed on disk" and similar news |
| [src/diff.rs](src/diff.rs) | Line diff, so a reload replaces only the lines that changed |
| [src/wait.rs](src/wait.rs) | `zpp --wait` |
| [src/languages.rs](src/languages.rs) | Highlighted languages: lexer, file names, keywords, token kinds |
| [src/syntax_colors.rs](src/syntax_colors.rs) | Code colors from the Notepad++ themes (generated) |
| [build.rs](build.rs) | Compiles Scintilla and Lexilla from [vendor](vendor) into the exe |
| [src/file.rs](src/file.rs) | Reading and writing encodings and line endings, safe save |
| [src/themes.rs](src/themes.rs) | Theme presets and color math |
| [src/theme.rs](src/theme.rs) | Applying colors and fonts to the window, dark menu bar painting |
| [src/fonts.rs](src/fonts.rs) | Bundled fonts and the font list |
| [src/prefs.rs](src/prefs.rs) | Settings in the registry |

Dark menus rely on two undocumented uxtheme.dll functions, the same approach Notepad++ uses. If a Windows update removes them, the menus fall back to light and everything else keeps working.

## Theme colors

Each theme has a text color, a background color and seven code colors (comments, keywords, types, strings, numbers, operators and preprocessor lines). For the 22 themes that ship with [Notepad++](https://github.com/notepad-plus-plus/notepad-plus-plus/tree/master/PowerEditor/installer/themes), the text and background come from each theme file's Default Style, and the code colors come from its C++ styles. The Light theme's code colors come from Notepad++'s default styles ([stylers.model.xml](https://github.com/notepad-plus-plus/notepad-plus-plus/blob/master/PowerEditor/src/stylers.model.xml)). [scripts/import_npp_themes.py](scripts/import_npp_themes.py) and [scripts/import_npp_syntax.py](scripts/import_npp_syntax.py) read them. No theme file is copied into z++.

The other themes and Custom get code colors computed from their text and background: a fixed hue for each kind of token, made light or dark enough to keep a 4.5:1 contrast with the background. Green Screen and High Contrast keep their one text color and use bold and italic instead.

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

The z++ source code is licensed under the [Apache License 2.0](LICENSE). Scintilla and Lexilla, in [vendor](vendor), keep their own license ([License.txt](vendor/scintilla/License.txt)); [vendor/PATCHES.md](vendor/PATCHES.md) lists the versions and the one change z++ makes to them. The bundled fonts keep their own licenses, listed above. See [NOTICE](NOTICE).

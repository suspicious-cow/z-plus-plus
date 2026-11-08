# Changelog

Notable changes to z++. The format follows [Keep a Changelog](https://keepachangelog.com/en/1.1.0/), and versions follow [Semantic Versioning](https://semver.org/).

## [Unreleased]

### Added

- `zpp file:42` and `zpp file:42:7` open a file at a line and column, the form compilers and coding agents print.
- `zpp --wait` returns only once the file's tab is closed, so z++ can be the editor for git and other tools.
- Files changed by another program now reload without waiting for you to switch back to z++. Only the lines that differ are replaced, so the caret and scroll position stay put and Undo goes back. A margin marks changed lines.
- When a tab with unsaved edits changes on disk, a bar above the text offers Reload or Keep mine instead of a dialog. A file deleted on disk gets the same kind of bar.
- Syntax highlighting for 24 languages, chosen by file name, with a Language menu to switch a tab to another language or plain text. Each theme colors code its own way: the Notepad++ themes use their own code colors, and the rest get colors computed to stay readable on their background.
- Line numbers, on by default (View > Line Numbers).
- Regular expressions in Find and Replace (ECMAScript syntax), with `\1` to `\9` in the replacement for captured groups. An invalid expression says so instead of finding nothing.
- Tabs, one per open file, each with its own undo history. Ctrl+W closes a tab, Ctrl+Tab switches, a dot marks unsaved changes.
- One z++ window: files opened from Explorer while z++ runs open as tabs there. `--new-window` opens a separate window.
- Find, Find Next/Previous (F3, Shift+F3), Replace, Replace All as one undo step, and Go to Line (Ctrl+G).
- A status bar with line and column, selection size, zoom, line endings and encoding; clicking the last two changes them.
- Format > Encoding and Format > Line Endings convert the file on its next save.
- File > Recent Files, Save All, opening several files at once, and dragging files onto the window.
- z++ reopens last time's files and window position.
- If z++ ever crashes, it says why and saves the report to `%TEMP%\zpp-crash.txt`.

### Changed

- The text is now edited with Scintilla 5.6.9 instead of the Windows RichEdit control. Scintilla and Lexilla are compiled into the exe, so it is still one file with nothing to install.
- Find and Replace is one dialog with Find Next, Find Previous, Replace and Replace All buttons.
- Format > Line Endings converts the text in place as one undo step.
- Saving writes a temporary file and swaps it in, so an interrupted save cannot leave a half-written file.

- 19 more themes, so z++ now has every theme that ships with Notepad++ (their default text and background colors), 32 in all.
- Live theme preview: pointing at a theme in View > Theme or the toolbar's theme list shows it in the whole window; Escape or moving away goes back.
- The theme list draws each theme in its own colors, and View > Theme groups light and dark themes in columns.
- An app icon, shown in the title bar, taskbar, Start menu and Explorer.
- Version and publisher details in the exe's Properties > Details.
- The installer registers z++ for the file types Windows Notepad and Notepad++ open (91 in all), lists z++ in Settings > Default apps, and makes z++ the default for types that have no default app.
- `install.ps1`, a per-user installer: Start menu shortcut, Win+R, "Open with", "Edit with z++", and removal from Settings > Apps. Release zips include it.

## [0.2.0] - 2026-11-03

### Added

- 13 color themes and a Custom theme with your own text and background colors.
- Ctrl+Shift+D switches between the last light and dark theme you used.
- 11 open-licensed monospace fonts built into the exe, a Font menu with the common Windows monospace fonts, and the Windows font dialog for any installed font.
- Font size controls: Ctrl+=, Ctrl+- and Ctrl+0.
- A toolbar under the menu bar with file, edit, font and view commands. View > Toolbar hides it.
- Help > Font Licenses and Help > About.
- The `bundled-fonts` build feature, on by default.

## [0.1.0] - 2026-11-03

### Added

- First version: New, Open, Save, Save As, undo, redo and clipboard commands, word wrap, and a light and dark mode.
- Native builds for Windows ARM64 and x64 with a statically linked C runtime.
- Keeps UTF-8, UTF-8 BOM, UTF-16 LE and ANSI encodings and LF line endings, and refuses files it could not save back unchanged.

[Unreleased]: https://github.com/suspicious-cow/z-plus-plus/compare/v0.2.0...HEAD
[0.2.0]: https://github.com/suspicious-cow/z-plus-plus/releases/tag/v0.2.0
[0.1.0]: https://github.com/suspicious-cow/z-plus-plus/commit/c7655e2

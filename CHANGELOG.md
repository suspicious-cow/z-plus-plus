# Changelog

Notable changes to z++. The format follows [Keep a Changelog](https://keepachangelog.com/en/1.1.0/), and versions follow [Semantic Versioning](https://semver.org/).

## [Unreleased]

### Added

- An app icon, shown in the title bar, taskbar, Start menu and Explorer.
- Version and publisher details in the exe's Properties > Details.
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

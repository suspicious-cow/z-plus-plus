# Changelog

Notable changes to z++. The format follows [Keep a Changelog](https://keepachangelog.com/en/1.1.0/), and versions follow [Semantic Versioning](https://semver.org/).

## [Unreleased]

### Fixed

- The "changed on disk" dialog could open off screen while z++ was minimized, leaving z++ disabled behind it and looking frozen. The dialog now waits until z++ is restored and in front, centers on the z++ window (nudged only to stay on its monitor), and comes to the front. It is short, with two large buttons: Reload (the default, Enter) and Keep current, or Reload (lose my edits) and Keep mine (the default) when the tab has unsaved edits. A long file name is shortened so the buttons always show. Message boxes also restore a minimized z++ first.
- Picking a text or background color colored the whole window, toolbar, tabs, and status bar included. Those colors (now View > Editor Text Color and Editor Background Color, the Custom theme) change only the editor; the window keeps Light or Dark to match.

### Added

- Text color and highlight for the selection (Format > Text Color, Highlight Color, and Remove Color, or the two color buttons in the toolbar's Font group) in Markdown and HTML files. They are saved as `<span style="color:#rrggbb">` and `<span style="background-color:#rrggbb">`, a second color replaces the first, and a selection over several lines gets a span per line. The editor shows the colors in the source and in Rendered Markdown, where the tags are hidden like other markup. Other files can't store color, so the items are dimmed there and do nothing.
- A mini toolbar over mouse selections, like Word's, that never takes focus. Markdown: Bold, Italic, Underline, Strikethrough, Code, Link (`[text](url)` with `url` selected), Heading, List, and Quote. Other files: Cut, Copy, UPPERCASE, lowercase. Typing, keyboard selection, scrolling, switching tabs, or moving the pointer away hides it.
- Toolbar icons are colored by group (file, undo, clipboard, find, format, view), each checked for 4.5:1 contrast on every theme; Green Screen and High Contrast stay one color.
- Bold, Italic, and Underline for Markdown files: Format menu, B, I, and U buttons in the toolbar's Font group, and Ctrl+B, Ctrl+I, Ctrl+U. They wrap the selection (or the word at the caret) in `**`, `*`, or `<u>...</u>` (Markdown has no underline), toggle off when pressed again, and nest bold with italic as `***`. Each is one undo step. In other files the buttons and menu items are dimmed, and only the Ctrl+U key keeps lowercasing.
- Tab control: drag a tab to move it; Ctrl+click and Shift+click select several tabs and a drag moves them together. Right-click a tab for Close (Close N Tabs for a selection), Close Others, and Close Tabs to the Right; unsaved tabs still ask, and Cancel stops the rest. Ctrl+Shift+PgUp and Ctrl+Shift+PgDn (View > Move Tab Left, Right) move the selected tabs one place. The saved session follows the new order.
- Rendered Markdown (View > Rendered Markdown, Ctrl+Shift+M, or the Rendered button on the toolbar): a Markdown tab shows its text formatted and stays editable. Markup is hidden until the caret is inside it, and nothing is converted, so text you don't edit keeps its exact bytes and Save writes the same file.
- Find in Files (Ctrl+Shift+F) with a file filter, match case, whole word and regular expressions. It searches on a background thread, skips build and dependency folders and binary files, and lists hits in a panel under the text; double-click a hit to open it. View > Search Results hides the panel.
- `zpp file:42` and `zpp file:42:7` open a file at a line and column, the form compilers and coding agents print.
- `zpp --wait` returns only once the file's tab is closed, so z++ can be the editor for git and other tools.
- When another program changes or deletes an open file, z++ asks in a modal dialog drawn in its own colors, centered on its window. A deleted file offers Keep it open, Save it again, or Close the tab. Each change is asked about once; in the background z++ flashes its taskbar button and asks when you return.
- A reload replaces only the lines that differ, so the caret and scroll position stay put and Undo goes back. A margin marks changed lines.
- `ZPP_PROFILE=<name>` runs z++ with its own settings and its own window, separate from your everyday z++. The GUI tests use it, so they never touch your settings or open files.
- Syntax highlighting for 24 languages, chosen by file name, with a Language menu to switch a tab to another language or plain text. Each theme colors code its own way: the Notepad++ themes use their own code colors, and the rest get colors computed to stay readable on their background.
- Line numbers, on by default (View > Line Numbers).
- Regular expressions in Find and Replace (ECMAScript syntax), with `\1` to `\9` in the replacement for captured groups. An invalid expression says so instead of finding nothing.
- Tabs, one per open file, each with its own undo history. Ctrl+W closes a tab, Ctrl+Tab switches, a dot marks unsaved changes.
- One z++ window: files opened from Explorer while z++ runs open as tabs there. `--new-window` opens a separate window.
- Find, Find Next/Previous (F3, Shift+F3), Replace, Replace All as one undo step, and Go to Line (Ctrl+G).
- A status bar with line and column, selection size, zoom, line endings and encoding; clicking the last two changes them.
- Format > Encoding and Format > Line Endings convert the file on its next save.
- File > Recent Files, Save All, opening several files at once, and dragging files onto the window.
- z++ reopens last time's tabs in order, with the active tab, caret, scroll position, and rendered view, and the window position. The tab list is saved every couple of seconds while it changes, so a crash or forced restart no longer loses it. A tab you close drops out; a file deleted since is skipped with a note in the status bar.
- Unsaved edits survive closing z++, like Notepad++: they are kept in backup files under `%LOCALAPPDATA%\z++\backup` and come back unsaved next time (Undo returns to the file's text). File > Remember Unsaved Changes turns this off and brings back the Save prompt on exit.
- File > Recent Files keeps 20 files, newest first. Reopening a file moves it to the top, and other spellings of the same path (forward slashes, short names, case) count as one file. A second window no longer overwrites the list or the session.
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

- A mini toolbar over mouse selections, like Word's, that never takes focus. Markdown: Bold, Italic, Underline, Strikethrough, Code, Link (`[text](url)` with `url` selected), Heading, List, and Quote. Other files: Cut, Copy, UPPERCASE, lowercase. Typing, keyboard selection, scrolling, switching tabs, or moving the pointer away hides it.
- Toolbar icons are colored by group (file, undo, clipboard, find, format, view), each checked for 4.5:1 contrast on every theme; Green Screen and High Contrast stay one color.
- Bold, Italic, and Underline for Markdown files: Format menu, B, I, and U buttons in the toolbar's Font group, and Ctrl+B, Ctrl+I, Ctrl+U. They wrap the selection (or the word at the caret) in `**`, `*`, or `<u>...</u>` (Markdown has no underline), toggle off when pressed again, and nest bold with italic as `***`. Each is one undo step. In other files the buttons and menu items are dimmed, and only the Ctrl+U key keeps lowercasing.
- Tab control: drag a tab to move it; Ctrl+click and Shift+click select several tabs and a drag moves them together. Right-click a tab for Close (Close N Tabs for a selection), Close Others, and Close Tabs to the Right; unsaved tabs still ask, and Cancel stops the rest. Ctrl+Shift+PgUp and Ctrl+Shift+PgDn (View > Move Tab Left, Right) move the selected tabs one place. The saved session follows the new order.
- 13 color themes and a Custom theme with your own text and background colors.
- Ctrl+Shift+D switches between the last light and dark theme you used.
- 11 open-licensed monospace fonts built into the exe, a Font menu with the common Windows monospace fonts, and the Windows font dialog for any installed font.
- Font size controls: Ctrl+=, Ctrl+- and Ctrl+0.
- A toolbar under the menu bar with file, edit, font and view commands. View > Toolbar hides it.
- Help > Font Licenses and Help > About.
- The `bundled-fonts` build feature, on by default.

## [0.1.0] - 2026-11-03

### Added

- A mini toolbar over mouse selections, like Word's, that never takes focus. Markdown: Bold, Italic, Underline, Strikethrough, Code, Link (`[text](url)` with `url` selected), Heading, List, and Quote. Other files: Cut, Copy, UPPERCASE, lowercase. Typing, keyboard selection, scrolling, switching tabs, or moving the pointer away hides it.
- Toolbar icons are colored by group (file, undo, clipboard, find, format, view), each checked for 4.5:1 contrast on every theme; Green Screen and High Contrast stay one color.
- Bold, Italic, and Underline for Markdown files: Format menu, B, I, and U buttons in the toolbar's Font group, and Ctrl+B, Ctrl+I, Ctrl+U. They wrap the selection (or the word at the caret) in `**`, `*`, or `<u>...</u>` (Markdown has no underline), toggle off when pressed again, and nest bold with italic as `***`. Each is one undo step. In other files the buttons and menu items are dimmed, and only the Ctrl+U key keeps lowercasing.
- Tab control: drag a tab to move it; Ctrl+click and Shift+click select several tabs and a drag moves them together. Right-click a tab for Close (Close N Tabs for a selection), Close Others, and Close Tabs to the Right; unsaved tabs still ask, and Cancel stops the rest. Ctrl+Shift+PgUp and Ctrl+Shift+PgDn (View > Move Tab Left, Right) move the selected tabs one place. The saved session follows the new order.
- First version: New, Open, Save, Save As, undo, redo and clipboard commands, word wrap, and a light and dark mode.
- Native builds for Windows ARM64 and x64 with a statically linked C runtime.
- Keeps UTF-8, UTF-8 BOM, UTF-16 LE and ANSI encodings and LF line endings, and refuses files it could not save back unchanged.

[Unreleased]: https://github.com/suspicious-cow/z-plus-plus/compare/v0.2.0...HEAD
[0.2.0]: https://github.com/suspicious-cow/z-plus-plus/releases/tag/v0.2.0
[0.1.0]: https://github.com/suspicious-cow/z-plus-plus/commit/c7655e2

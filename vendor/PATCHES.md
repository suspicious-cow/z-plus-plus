# Vendored libraries

z++ compiles these into zpp.exe (see build.rs). Only the folders needed for the Windows build are kept.

| Library | Version | Source | Checked |
| --- | --- | --- | --- |
| Scintilla | 5.6.9 | https://www.scintilla.org/scintilla569.tgz | Same files as tag `rel-5-6-9` in the official Mercurial repo (http://hg.code.sf.net/p/scintilla/code) |
| Lexilla | 5.5.5 | https://www.scintilla.org/lexilla555.tgz | Same files as tag `rel-5-5-5` in https://github.com/ScintillaOrg/lexilla |

Both are under the license in their `License.txt`. Git stores every file with LF line endings (see `.gitattributes`), so a few upstream files that use CRLF differ from the tarballs in line endings only.

## Changes to the upstream source

`scintilla/win32/ScintillaWin.cxx`: Scintilla accepts only dragged text, so files dragged from Explorer onto the text were refused. The patch (marked `z++ patch`) accepts `CF_HDROP` in `DragEnter`, `DragOver` and `Drop`, and sends the drop handle to the parent window as `WM_APP + 7`. The parent opens the files and must not call `DragFinish`.

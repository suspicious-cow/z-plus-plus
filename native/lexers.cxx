// z++'s lexer catalogue: CreateLexer for only the lexers src/languages.rs uses.
//
// Lexilla's own catalogue (vendor/lexilla/src/Lexilla.cxx) references all of its lexers, so
// every one ends up in the exe. Listing just ours lets the linker leave the rest out.
// Add a lexer here when a language in src/languages.rs starts using it; the languages tests
// check that every one resolves.

#include <cstring>

#include <initializer_list>
#include <vector>

#include "ILexer.h"

#include "LexerModule.h"
#include "CatalogueModules.h"

using namespace Lexilla;

extern const LexerModule lmBash;
extern const LexerModule lmBatch;
extern const LexerModule lmCPP;
extern const LexerModule lmCss;
extern const LexerModule lmDiff;
extern const LexerModule lmHTML;
extern const LexerModule lmJSON;
extern const LexerModule lmLua;
extern const LexerModule lmMake;
extern const LexerModule lmMarkdown;
extern const LexerModule lmPowerShell;
extern const LexerModule lmProps;
extern const LexerModule lmPython;
extern const LexerModule lmRust;
extern const LexerModule lmSQL;
extern const LexerModule lmTOML;
extern const LexerModule lmXML;
extern const LexerModule lmYAML;

namespace {

CatalogueModules catalogue;

void AddLexers() {
	if (catalogue.Count() == 0) {
		catalogue.AddLexerModules({
			&lmBash, &lmBatch, &lmCPP, &lmCss, &lmDiff, &lmHTML, &lmJSON, &lmLua, &lmMake,
			&lmMarkdown, &lmPowerShell, &lmProps, &lmPython, &lmRust, &lmSQL, &lmTOML,
			&lmXML, &lmYAML,
		});
	}
}

}

extern "C" Scintilla::ILexer5 *__stdcall CreateLexer(const char *name) {
	AddLexers();
	for (size_t i = 0; i < catalogue.Count(); i++) {
		if (0 == strcmp(catalogue.Name(i), name)) {
			return catalogue.Create(i);
		}
	}
	return nullptr;
}

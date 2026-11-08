//! Languages z++ highlights: which Lexilla lexer each one uses, the file names it applies to,
//! its keywords, and which kind of token each of the lexer's styles is.

use std::path::Path;

/// Kinds of token a theme colors (see themes::Syntax). Styles not listed use the text color.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Token {
    Comment,
    Keyword,
    Kind,
    Str,
    Number,
    Operator,
    Preproc,
}

use Token::*;

pub struct Language {
    pub name: &'static str,
    /// The Lexilla lexer name.
    pub lexer: &'static str,
    /// Lowercase extensions without the dot.
    pub extensions: &'static [&'static str],
    /// Whole file names (lowercase) for files without an extension, such as Makefile.
    pub file_names: &'static [&'static str],
    /// Keyword lists, in the lexer's order. SQL, Batch and PowerShell lexers want lowercase.
    pub keywords: &'static [&'static str],
    pub properties: &'static [(&'static str, &'static str)],
    pub styles: &'static [(u8, Token)],
}

const CPP_STYLES: &[(u8, Token)] = &[
    (1, Comment),
    (2, Comment),
    (3, Comment),
    (15, Comment),
    (17, Comment),
    (18, Comment),
    (23, Comment),
    (24, Comment),
    (5, Keyword),
    (16, Kind),
    (19, Kind),
    (6, Str),
    (7, Str),
    (12, Str),
    (13, Str),
    (14, Str),
    (20, Str),
    (21, Str),
    (22, Str),
    (4, Number),
    (27, Number),
    (10, Operator),
    (9, Preproc),
];

/// HTML and XML, plus JavaScript inside HTML <script> blocks.
const MARKUP_STYLES: &[(u8, Token)] = &[
    (9, Comment),
    (20, Comment),
    (29, Comment),
    (1, Keyword),
    (2, Keyword),
    (11, Keyword),
    (12, Preproc),
    (13, Preproc),
    (3, Kind),
    (4, Kind),
    (6, Str),
    (7, Str),
    (17, Str),
    (19, Str),
    (5, Number),
    (10, Number),
    (21, Preproc),
    (22, Preproc),
    (23, Preproc),
    (24, Str),
    (25, Str),
    (28, Number),
    (42, Comment),
    (43, Comment),
    (44, Comment),
    (45, Number),
    (46, Keyword),
    (47, Keyword),
    (48, Str),
    (49, Str),
    (51, Str),
    (52, Str),
    (53, Str),
    (50, Operator),
];

const C_KEYWORDS: &str = "alignas alignof auto bool break case char const constexpr continue default do double else enum extern false float for goto if inline int long nullptr register restrict return short signed sizeof static static_assert struct switch thread_local true typedef typeof union unsigned void volatile while _Alignas _Alignof _Atomic _Bool _Complex _Generic _Imaginary _Noreturn _Static_assert _Thread_local";
const C_TYPES: &str = "size_t ssize_t ptrdiff_t intptr_t uintptr_t int8_t int16_t int32_t int64_t uint8_t uint16_t uint32_t uint64_t wchar_t FILE NULL";
const CPP_KEYWORDS: &str = "alignas alignof and and_eq asm auto bitand bitor bool break case catch char char8_t char16_t char32_t class co_await co_return co_yield compl concept const consteval constexpr constinit const_cast continue decltype default delete do double dynamic_cast else enum explicit export extern false final float for friend goto if import inline int long module mutable namespace new noexcept not not_eq nullptr operator or or_eq override private protected public register reinterpret_cast requires return short signed sizeof static static_assert static_cast struct switch template this thread_local throw true try typedef typeid typename union unsigned using virtual void volatile wchar_t while xor xor_eq";
const CPP_TYPES: &str = "size_t ptrdiff_t intptr_t uintptr_t int8_t int16_t int32_t int64_t uint8_t uint16_t uint32_t uint64_t std string string_view vector map unordered_map set array optional variant unique_ptr shared_ptr weak_ptr";
const CS_KEYWORDS: &str = "abstract as async await base bool break byte case catch char checked class const continue decimal default delegate do double dynamic else enum event explicit extern false finally fixed float for foreach get goto if implicit in init int interface internal is lock long nameof namespace new null object operator out override params partial private protected public readonly record ref return sbyte sealed set short sizeof stackalloc static string struct switch this throw true try typeof uint ulong unchecked unsafe ushort using value var virtual void volatile when where while yield";
const JAVA_KEYWORDS: &str = "abstract assert boolean break byte case catch char class const continue default do double else enum extends false final finally float for goto if implements import instanceof int interface long native new non-sealed null package permits private protected public record return sealed short static strictfp super switch synchronized this throw throws transient true try var void volatile while yield";
const JS_KEYWORDS: &str = "async await break case catch class const continue debugger default delete do else export extends false finally for from function get if import in instanceof let new null of return set static super switch this throw true try typeof undefined var void while with yield";
const TS_KEYWORDS: &str = "abstract any as async await bigint boolean break case catch class const continue debugger declare default delete do else enum export extends false finally for from function get if implements import in infer instanceof interface is keyof let namespace never new null number of private protected public readonly return satisfies set static string super switch symbol this throw true try type typeof undefined unknown var void while with yield";
const GO_KEYWORDS: &str = "break case chan const continue default defer else fallthrough false for func go goto if import interface iota map nil package range return select struct switch true type var";
const GO_TYPES: &str = "any bool byte comparable complex64 complex128 error float32 float64 int int8 int16 int32 int64 rune string uint uint8 uint16 uint32 uint64 uintptr";
const PYTHON_KEYWORDS: &str = "False None True and as assert async await break case class continue def del elif else except finally for from global if import in is lambda match nonlocal not or pass raise return try while with yield";
const PYTHON_BUILTINS: &str = "abs all any bool bytes callable chr dict dir divmod enumerate filter float format frozenset getattr hasattr hash help hex id input int isinstance issubclass iter len list map max min next object oct open ord pow print property range repr reversed round self set setattr slice sorted str sum super tuple type vars zip";
const RUST_KEYWORDS: &str = "as async await break const continue crate dyn else enum extern false fn for if impl in let loop match mod move mut pub ref return self Self static struct super trait true type union unsafe use where while yield";
const RUST_TYPES: &str = "bool char str u8 u16 u32 u64 u128 usize i8 i16 i32 i64 i128 isize f32 f64 String Vec Option Result Box Some None Ok Err";
const SQL_KEYWORDS: &str = "add all alter and any as asc begin between by case check column commit constraint create cross database default delete desc distinct drop else end except exists foreign from full grant group having if in index inner insert intersect into is join key left like limit merge natural not null offset on or order outer primary procedure references returning revoke right rollback select set table then top transaction trigger truncate union unique update using values view when where with";
const SQL_TYPES: &str = "bigint binary bit blob boolean char date datetime decimal double float int integer interval json money nchar numeric nvarchar real serial smallint text time timestamp tinyint uuid varbinary varchar";
const POWERSHELL_KEYWORDS: &str = "begin break catch class clean continue data default define do dynamicparam else elseif end enum exit filter finally for foreach from function hidden if in param process return static switch throw trap try until using var while workflow";
const BASH_KEYWORDS: &str = "alias break case cd continue declare do done echo elif else esac eval exec exit export false fi for function if in local printf read readonly return select set shift source then time trap true typeset unset until while";
const BATCH_KEYWORDS: &str = "assoc break call cd chcp chdir cls color copy date defined del dir do echo else endlocal equ erase errorlevel exist exit for ftype geq goto gtr if in leq lss md mkdir move neq not nul path pause popd prompt pushd rd rem ren rename rmdir set setlocal shift start time title type ver verify vol";
const LUA_KEYWORDS: &str = "and break do else elseif end false for function goto if in local nil not or repeat return then true until while";
const JSON_KEYWORDS: &str = "false null true";
const YAML_KEYWORDS: &str = "false no null off on true yes";
const TOML_KEYWORDS: &str = "false inf nan true";

const BACKQUOTE_STRINGS: &[(&str, &str)] = &[("lexer.cpp.backquoted.strings", "1")];

const fn cpp(
    name: &'static str,
    extensions: &'static [&'static str],
    keywords: &'static [&'static str],
    properties: &'static [(&'static str, &'static str)],
) -> Language {
    Language {
        name,
        lexer: "cpp",
        extensions,
        file_names: &[],
        keywords,
        properties,
        styles: CPP_STYLES,
    }
}

const fn other(
    name: &'static str,
    lexer: &'static str,
    extensions: &'static [&'static str],
    keywords: &'static [&'static str],
    styles: &'static [(u8, Token)],
) -> Language {
    Language {
        name,
        lexer,
        extensions,
        file_names: &[],
        keywords,
        properties: &[],
        styles,
    }
}

/// In the order the Language menu lists them.
pub const LANGUAGES: &[Language] = &[
    other(
        "Batch",
        "batch",
        &["bat", "cmd"],
        &[BATCH_KEYWORDS],
        &[
            (1, Comment),
            (2, Keyword),
            (3, Preproc),
            (8, Preproc),
            (4, Operator),
            (5, Kind),
            (6, Number),
            (7, Operator),
        ],
    ),
    cpp("C", &["c", "h"], &[C_KEYWORDS, C_TYPES], &[]),
    cpp("C#", &["cs", "csx"], &[CS_KEYWORDS], &[]),
    cpp(
        "C++",
        &[
            "cpp", "cc", "cxx", "c++", "hpp", "hh", "hxx", "h++", "ino", "ipp", "inl",
        ],
        &[CPP_KEYWORDS, CPP_TYPES],
        &[],
    ),
    other(
        "CSS",
        "css",
        &["css", "scss", "less"],
        &[],
        &[
            (9, Comment),
            (1, Keyword),
            (2, Keyword),
            (10, Keyword),
            (11, Keyword),
            (3, Preproc),
            (4, Preproc),
            (18, Preproc),
            (20, Preproc),
            (21, Preproc),
            (12, Preproc),
            (22, Preproc),
            (6, Kind),
            (7, Kind),
            (15, Kind),
            (17, Kind),
            (19, Kind),
            (23, Kind),
            (8, Number),
            (13, Str),
            (14, Str),
            (5, Operator),
        ],
    ),
    other(
        "Diff",
        "diff",
        &["diff", "patch"],
        &[],
        &[
            (1, Comment),
            (2, Keyword),
            (3, Kind),
            (4, Number),
            (5, Preproc),
            (9, Preproc),
            (11, Preproc),
            (6, Str),
            (8, Str),
            (10, Str),
            (7, Kind),
        ],
    ),
    cpp("Go", &["go"], &[GO_KEYWORDS, GO_TYPES], BACKQUOTE_STRINGS),
    other(
        "HTML",
        "hypertext",
        &["html", "htm", "xhtml", "shtml", "hta"],
        &["", JS_KEYWORDS],
        MARKUP_STYLES,
    ),
    other(
        "INI",
        "props",
        &[
            "ini",
            "cfg",
            "conf",
            "properties",
            "inf",
            "editorconfig",
            "gitconfig",
            "gitattributes",
            "gitignore",
        ],
        &[],
        &[
            (1, Comment),
            (2, Keyword),
            (5, Kind),
            (3, Operator),
            (4, Str),
        ],
    ),
    cpp("Java", &["java"], &[JAVA_KEYWORDS], &[]),
    cpp(
        "JavaScript",
        &["js", "mjs", "cjs", "jsx"],
        &[JS_KEYWORDS],
        BACKQUOTE_STRINGS,
    ),
    other(
        "JSON",
        "json",
        &["json", "jsonc", "json5", "webmanifest"],
        &[JSON_KEYWORDS],
        &[
            (6, Comment),
            (7, Comment),
            (11, Keyword),
            (12, Keyword),
            (4, Kind),
            (2, Str),
            (3, Str),
            (1, Number),
            (5, Number),
            (8, Operator),
        ],
    ),
    other(
        "Lua",
        "lua",
        &["lua"],
        &[LUA_KEYWORDS],
        &[
            (1, Comment),
            (2, Comment),
            (3, Comment),
            (5, Keyword),
            (13, Kind),
            (14, Kind),
            (6, Str),
            (7, Str),
            (8, Str),
            (12, Str),
            (4, Number),
            (10, Operator),
            (9, Preproc),
            (20, Preproc),
        ],
    ),
    Language {
        name: "Makefile",
        lexer: "makefile",
        extensions: &["mak", "mk"],
        file_names: &["makefile", "gnumakefile"],
        keywords: &[],
        properties: &[],
        styles: &[
            (1, Comment),
            (2, Preproc),
            (3, Kind),
            (4, Operator),
            (5, Keyword),
            (9, Kind),
        ],
    },
    other(
        "Markdown",
        "markdown",
        &["md", "markdown", "mdown", "mkd"],
        &[],
        &[
            (6, Keyword),
            (7, Keyword),
            (8, Keyword),
            (9, Keyword),
            (10, Keyword),
            (11, Keyword),
            (2, Kind),
            (3, Kind),
            (4, Kind),
            (5, Kind),
            (16, Comment),
            (15, Comment),
            (13, Operator),
            (14, Operator),
            (17, Operator),
            (18, Number),
            (19, Str),
            (20, Str),
            (21, Str),
            (22, Preproc),
            (23, Preproc),
            (24, Preproc),
        ],
    ),
    other(
        "PowerShell",
        "powershell",
        &["ps1", "psm1", "psd1"],
        &[POWERSHELL_KEYWORDS],
        &[
            (1, Comment),
            (13, Comment),
            (16, Comment),
            (8, Keyword),
            (9, Kind),
            (10, Kind),
            (11, Kind),
            (2, Str),
            (3, Str),
            (14, Str),
            (15, Str),
            (4, Number),
            (5, Preproc),
            (6, Operator),
        ],
    ),
    other(
        "Python",
        "python",
        &["py", "pyw", "pyi"],
        &[PYTHON_KEYWORDS, PYTHON_BUILTINS],
        &[
            (1, Comment),
            (12, Comment),
            (5, Keyword),
            (14, Kind),
            (8, Kind),
            (9, Kind),
            (3, Str),
            (4, Str),
            (6, Str),
            (7, Str),
            (13, Str),
            (16, Str),
            (17, Str),
            (18, Str),
            (19, Str),
            (2, Number),
            (10, Operator),
            (15, Preproc),
        ],
    ),
    other(
        "Rust",
        "rust",
        &["rs"],
        &[RUST_KEYWORDS, RUST_TYPES],
        &[
            (1, Comment),
            (2, Comment),
            (3, Comment),
            (4, Comment),
            (6, Keyword),
            (7, Kind),
            (18, Kind),
            (13, Str),
            (14, Str),
            (15, Str),
            (21, Str),
            (22, Str),
            (23, Str),
            (24, Str),
            (25, Str),
            (5, Number),
            (16, Operator),
            (19, Preproc),
        ],
    ),
    other(
        "Shell",
        "bash",
        &["sh", "bash", "zsh", "ksh"],
        &[BASH_KEYWORDS],
        &[
            (2, Comment),
            (4, Keyword),
            (5, Str),
            (6, Str),
            (11, Str),
            (12, Str),
            (13, Str),
            (3, Number),
            (7, Operator),
            (9, Preproc),
            (10, Preproc),
        ],
    ),
    other(
        "SQL",
        "sql",
        &["sql"],
        &[SQL_KEYWORDS, SQL_TYPES],
        &[
            (1, Comment),
            (2, Comment),
            (3, Comment),
            (13, Comment),
            (15, Comment),
            (17, Comment),
            (18, Comment),
            (5, Keyword),
            (16, Kind),
            (6, Str),
            (7, Str),
            (23, Str),
            (4, Number),
            (10, Operator),
        ],
    ),
    other(
        "TOML",
        "toml",
        &["toml"],
        &[TOML_KEYWORDS],
        &[
            (1, Comment),
            (3, Keyword),
            (5, Keyword),
            (6, Kind),
            (9, Str),
            (10, Str),
            (11, Str),
            (12, Str),
            (15, Str),
            (4, Number),
            (13, Number),
            (14, Number),
            (8, Operator),
        ],
    ),
    cpp(
        "TypeScript",
        &["ts", "tsx", "mts", "cts"],
        &[TS_KEYWORDS],
        BACKQUOTE_STRINGS,
    ),
    other(
        "XML",
        "xml",
        &[
            "xml", "xsd", "xsl", "xslt", "svg", "config", "csproj", "vbproj", "vcxproj", "props",
            "targets", "plist", "resx", "manifest", "xaml", "rss", "atom", "kml", "wsdl",
        ],
        &[],
        MARKUP_STYLES,
    ),
    other(
        "YAML",
        "yaml",
        &["yml", "yaml"],
        &[YAML_KEYWORDS],
        &[
            (1, Comment),
            (3, Keyword),
            (2, Kind),
            (4, Number),
            (5, Preproc),
            (6, Preproc),
            (7, Str),
            (9, Operator),
        ],
    ),
];

/// The language for a file, by its extension or its whole name. None means plain text.
pub fn for_path(path: &Path) -> Option<usize> {
    let name = path.file_name()?.to_string_lossy().to_lowercase();
    let extension = path
        .extension()
        .map(|e| e.to_string_lossy().to_lowercase())
        // Dot files like .gitignore have no extension to Path, but the name after the dot works.
        .or_else(|| name.strip_prefix('.').map(str::to_string));
    LANGUAGES.iter().position(|l| {
        l.file_names.contains(&name.as_str())
            || extension
                .as_deref()
                .is_some_and(|e| l.extensions.contains(&e))
    })
}

pub fn get(index: Option<usize>) -> Option<&'static Language> {
    LANGUAGES.get(index?)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn finds_languages_by_extension_and_name() {
        let name = |p: &str| get(for_path(Path::new(p))).map(|l| l.name);
        assert_eq!(name(r"C:\src\main.RS"), Some("Rust"));
        assert_eq!(name("notes.txt"), None);
        assert_eq!(name("Makefile"), Some("Makefile"));
        assert_eq!(name(".gitignore"), Some("INI"));
        assert_eq!(name("app.config"), Some("XML"));
        assert_eq!(name("README"), None);
    }

    #[test]
    fn extensions_belong_to_one_language_and_lists_are_sorted() {
        let mut seen = std::collections::HashSet::new();
        for l in LANGUAGES {
            for e in l.extensions.iter().chain(l.file_names) {
                assert!(seen.insert(*e), "{e} is listed twice");
                assert_eq!(*e, e.to_lowercase());
            }
        }
        let names: Vec<_> = LANGUAGES.iter().map(|l| l.name.to_lowercase()).collect();
        let mut sorted = names.clone();
        sorted.sort();
        assert_eq!(names, sorted);
    }

    #[test]
    fn lowercase_keywords_for_lexers_that_lowercase() {
        for l in LANGUAGES
            .iter()
            .filter(|l| ["sql", "batch", "powershell"].contains(&l.lexer))
        {
            for list in l.keywords {
                assert_eq!(*list, list.to_lowercase(), "{}", l.name);
            }
        }
    }
}

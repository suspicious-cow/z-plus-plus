use std::path::{Path, PathBuf};

const RESOURCE_TEMPLATE: &str = r#"#include <winver.h>

1 ICON "{icon}"

VS_VERSION_INFO VERSIONINFO
FILEVERSION {major},{minor},{patch},0
PRODUCTVERSION {major},{minor},{patch},0
FILEOS VOS_NT_WINDOWS32
FILETYPE VFT_APP
BEGIN
  BLOCK "StringFileInfo"
  BEGIN
    BLOCK "040904B0"
    BEGIN
      VALUE "CompanyName", "Zain Naboulsi"
      VALUE "FileDescription", "z++ text editor"
      VALUE "FileVersion", "{version}"
      VALUE "InternalName", "zpp"
      VALUE "LegalCopyright", "Copyright 2026 Zain Naboulsi. Apache License 2.0."
      VALUE "OriginalFilename", "zpp.exe"
      VALUE "ProductName", "z++"
      VALUE "ProductVersion", "{version}"
    END
  END
  BLOCK "VarFileInfo"
  BEGIN
    VALUE "Translation", 0x409, 1200
  END
END
"#;

const SCINTILLA_WIN32: [&str; 6] = [
    "HanjaDic.cxx",
    "ListBox.cxx",
    "PlatWin.cxx",
    "ScintillaWin.cxx",
    "SurfaceD2D.cxx",
    "SurfaceGDI.cxx",
];
const SYSTEM_LIBS: [&str; 7] = [
    "user32", "gdi32", "imm32", "ole32", "oleaut32", "advapi32", "msimg32",
];

fn main() {
    let dir = PathBuf::from(env("CARGO_MANIFEST_DIR"));
    embed_manifest(&dir);
    embed_resources(&dir);
    compile_scintilla(&dir.join("vendor"));
}

/// Scintilla (the editing control) and Lexilla (syntax highlighting), linked into zpp.exe.
fn compile_scintilla(vendor: &Path) {
    println!("cargo:rerun-if-changed=vendor");
    let sci = vendor.join("scintilla");
    let lex = vendor.join("lexilla");
    let mut files = cxx_files(&sci.join("src"));
    files.extend(SCINTILLA_WIN32.iter().map(|f| sci.join("win32").join(f)));
    files.extend(cxx_files(&lex.join("lexlib")));
    files.extend(cxx_files(&lex.join("lexers")));
    files.push(lex.join("src").join("Lexilla.cxx"));
    cc::Build::new()
        .cpp(true)
        .std("c++17")
        .files(files)
        .include(sci.join("include"))
        .include(sci.join("src"))
        .include(lex.join("include"))
        .include(lex.join("lexlib"))
        .define("UNICODE", None)
        .define("_UNICODE", None)
        .define("NDEBUG", None)
        // Lexilla is linked in, not a DLL: keep zpp.exe from exporting its functions.
        .define("LEXILLA_NO_EXPORT", None)
        .flag("/EHsc")
        .flag("/utf-8")
        .warnings(false)
        .compile("scintilla");
    for lib in SYSTEM_LIBS {
        println!("cargo:rustc-link-lib={lib}");
    }
}

fn cxx_files(dir: &Path) -> Vec<PathBuf> {
    let mut files: Vec<PathBuf> = std::fs::read_dir(dir)
        .unwrap_or_else(|e| panic!("read {}: {e}", dir.display()))
        .filter_map(|e| e.ok().map(|e| e.path()))
        .filter(|p| p.extension().is_some_and(|x| x == "cxx"))
        .collect();
    files.sort();
    files
}

/// The manifest goes in through the linker, so the .rc file must not add another one.
fn embed_manifest(dir: &Path) {
    let manifest = dir.join("zpp.manifest");
    println!("cargo:rerun-if-changed=zpp.manifest");
    println!("cargo:rustc-link-arg-bins=/MANIFEST:EMBED");
    println!(
        "cargo:rustc-link-arg-bins=/MANIFESTINPUT:{}",
        manifest.display()
    );
}

/// The app icon and the version info shown in the exe's Properties > Details.
fn embed_resources(dir: &Path) {
    let icon = dir.join("assets").join("zpp.ico");
    println!("cargo:rerun-if-changed=assets/zpp.ico");
    let rc = RESOURCE_TEMPLATE
        .replace("{icon}", &icon.display().to_string().replace('\\', "/"))
        .replace("{version}", &env("CARGO_PKG_VERSION"))
        .replace("{major}", &env("CARGO_PKG_VERSION_MAJOR"))
        .replace("{minor}", &env("CARGO_PKG_VERSION_MINOR"))
        .replace("{patch}", &env("CARGO_PKG_VERSION_PATCH"));
    let rc_path = PathBuf::from(env("OUT_DIR")).join("zpp.rc");
    std::fs::write(&rc_path, rc).expect("write zpp.rc");
    embed_resource::compile(&rc_path, embed_resource::NONE)
        .manifest_optional()
        .expect("compile zpp.rc");
}

fn env(name: &str) -> String {
    std::env::var(name).unwrap_or_else(|_| panic!("{name} is not set"))
}

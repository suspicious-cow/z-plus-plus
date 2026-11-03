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

fn main() {
    let dir = PathBuf::from(env("CARGO_MANIFEST_DIR"));
    embed_manifest(&dir);
    embed_resources(&dir);
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

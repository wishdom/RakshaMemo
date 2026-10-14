//! On Windows, embed the app icon and version details in the executable, so
//! Explorer, the taskbar, and Task Manager show RakshaMemo with its icon.

use std::{env, fs, path::PathBuf};

#[path = "src/icon.rs"]
#[allow(dead_code)]
mod icon;

fn main() {
    println!("cargo:rerun-if-changed=build.rs");
    println!("cargo:rerun-if-changed=src/icon.rs");
    if env::var("CARGO_CFG_TARGET_OS").as_deref() != Ok("windows") {
        return;
    }
    let out = PathBuf::from(env::var_os("OUT_DIR").expect("Cargo sets OUT_DIR"));
    let icon_path = out.join("rakshamemo.ico");
    fs::write(&icon_path, icon::ico()).expect("Could not write the icon");

    let version = env::var("CARGO_PKG_VERSION").expect("Cargo sets the version");
    let number = |name: &str| {
        env::var(name)
            .ok()
            .and_then(|value| value.parse::<u16>().ok())
            .unwrap_or(0)
    };
    let numeric = format!(
        "{},{},{},0",
        number("CARGO_PKG_VERSION_MAJOR"),
        number("CARGO_PKG_VERSION_MINOR"),
        number("CARGO_PKG_VERSION_PATCH")
    );
    let icon_literal = icon_path.display().to_string().replace('\\', "\\\\");
    let resource = format!(
        r#"1 ICON "{icon_literal}"
1 VERSIONINFO
FILEVERSION {numeric}
PRODUCTVERSION {numeric}
BEGIN
  BLOCK "StringFileInfo"
  BEGIN
    BLOCK "040904b0"
    BEGIN
      VALUE "FileDescription", "RakshaMemo"
      VALUE "ProductName", "RakshaMemo"
      VALUE "FileVersion", "{version}"
      VALUE "ProductVersion", "{version}"
      VALUE "OriginalFilename", "RakshaMemo.exe"
    END
  END
  BLOCK "VarFileInfo"
  BEGIN
    VALUE "Translation", 0x409, 1200
  END
END
"#
    );
    let resource_path = out.join("rakshamemo.rc");
    fs::write(&resource_path, resource).expect("Could not write the resource script");
    embed_resource::compile(&resource_path, embed_resource::NONE)
        .manifest_optional()
        .expect("Could not compile the Windows resources");
}

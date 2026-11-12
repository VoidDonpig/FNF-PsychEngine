//! On Windows, the .exe is made to tell what it is: the game's icon, its name and version (shown by Explorer, Task
//! Manager and the Game Bar) and its manifest go into its resources; the graphics card hints are exported.
use std::env::var;

fn main() {
    println!("cargo:rerun-if-changed=build.rs");
    if var("CARGO_CFG_TARGET_OS").as_deref() != Ok("windows") {
        return;
    }
    let here = var("CARGO_MANIFEST_DIR").expect("set by cargo");
    let file = |name: &str| format!("{here}/{name}").replace('\\', "/");
    for name in ["icon.ico", "windows.manifest"] {
        println!("cargo:rerun-if-changed={}", file(name));
    }
    let version = var("CARGO_PKG_VERSION").expect("set by cargo");
    let numbers = version.split(['.', '-']).take(3).map(|part| part.parse::<u16>().unwrap_or(0).to_string()).collect::<Vec<_>>().join(",");
    let name = "Friday Night Funkin': Psych Engine";
    // (numbers in place of windows.h's names, so that no SDK headers are needed: 24 RT_MANIFEST, 0x40004 VOS_NT_WINDOWS32,
    // 1 VFT_APP, 0x409 US English in Unicode)
    let script = format!(
        r#"1 ICON "{icon}"
1 24 "{manifest}"
1 VERSIONINFO
FILEVERSION {numbers},0
PRODUCTVERSION {numbers},0
FILEOS 0x40004
FILETYPE 0x1
BEGIN
  BLOCK "StringFileInfo"
  BEGIN
    BLOCK "040904b0"
    BEGIN
      VALUE "CompanyName", "ShadowMario"
      VALUE "FileDescription", "{name}"
      VALUE "FileVersion", "{version}"
      VALUE "InternalName", "funkin"
      VALUE "OriginalFilename", "funkin.exe"
      VALUE "ProductName", "{name}"
      VALUE "ProductVersion", "{version}"
    END
  END
  BLOCK "VarFileInfo"
  BEGIN
    VALUE "Translation", 0x409, 1200
  END
END
"#,
        icon = file("icon.ico"),
        manifest = file("windows.manifest"),
    );
    let out = format!("{}/funkin.rc", var("OUT_DIR").expect("set by cargo"));
    std::fs::write(&out, script).expect("the resource script is written");
    embed_resource::compile(&out, embed_resource::NONE).manifest_required().expect("the resources are compiled");
    if var("CARGO_CFG_TARGET_ENV").as_deref() == Ok("msvc") {
        for symbol in ["NvOptimusEnablement", "AmdPowerXpressRequestHighPerformance"] {
            println!("cargo:rustc-link-arg-bins=/EXPORT:{symbol}");
        }
    }
}

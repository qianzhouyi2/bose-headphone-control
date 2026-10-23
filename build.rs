use std::{env, fs, path::PathBuf, process::Command};

fn main() {
    println!("cargo:rerun-if-changed=assets/app.ico");
    println!("cargo:rerun-if-changed=build.rs");
    if env::var("CARGO_CFG_TARGET_OS").as_deref() != Ok("windows") {
        return;
    }
    let version = env::var("CARGO_PKG_VERSION").unwrap();
    let display_version = version.strip_suffix(".0").unwrap_or(&version);
    let numeric_version = format!(
        "{},{},{},0",
        env::var("CARGO_PKG_VERSION_MAJOR").unwrap(),
        env::var("CARGO_PKG_VERSION_MINOR").unwrap(),
        env::var("CARGO_PKG_VERSION_PATCH").unwrap()
    );
    println!("cargo:rustc-env=APP_DISPLAY_VERSION={display_version}");
    let out = PathBuf::from(env::var_os("OUT_DIR").unwrap());
    let icon = PathBuf::from(env::var_os("CARGO_MANIFEST_DIR").unwrap()).join("assets/app.ico");
    let resource = out.join("app.rc");
    fs::write(
        &resource,
        format!(
            r#"#pragma code_page(65001)
101 ICON "{}"
1 VERSIONINFO
FILEVERSION {numeric_version}
PRODUCTVERSION {numeric_version}
FILEFLAGSMASK 0x3fL
FILEFLAGS 0x0L
FILEOS 0x40004L
FILETYPE 0x1L
BEGIN
 BLOCK "StringFileInfo"
 BEGIN
  BLOCK "040904B0"
  BEGIN
   VALUE "FileDescription", "Bose Headphone Control\0"
   VALUE "FileVersion", "{version}\0"
   VALUE "InternalName", "bose-headphone-control\0"
   VALUE "OriginalFilename", "bose-headphone-control.exe\0"
   VALUE "ProductName", "Bose Headphone Control\0"
   VALUE "ProductVersion", "{display_version}\0"
  END
 END
 BLOCK "VarFileInfo"
 BEGIN
  VALUE "Translation", 0x409, 1200
 END
END
"#,
            icon.display().to_string().replace('\\', "/")
        ),
    )
    .expect("write Windows resources");
    let target_arch = env::var("CARGO_CFG_TARGET_ARCH").unwrap();
    let arch = match target_arch.as_str() {
        "aarch64" => "arm64",
        "x86" => "x86",
        _ => "x64",
    };
    let sdk_bin = PathBuf::from(
        env::var_os("ProgramFiles(x86)").unwrap_or_else(|| "C:/Program Files (x86)".into()),
    )
    .join("Windows Kits/10/bin");
    let mut versions: Vec<_> = fs::read_dir(sdk_bin)
        .into_iter()
        .flatten()
        .flatten()
        .map(|e| e.path())
        .collect();
    versions.sort();
    let compiler = env::var_os("RC")
        .map(PathBuf::from)
        .or_else(|| {
            versions
                .into_iter()
                .rev()
                .map(|v| v.join(arch).join("rc.exe"))
                .find(|p| p.is_file())
        })
        .unwrap_or_else(|| "rc.exe".into());
    let result = Command::new(compiler)
        .arg("/nologo")
        .arg("/fo")
        .arg(out.join("app.res"))
        .arg(resource)
        .status()
        .expect("Windows SDK Resource Compiler (rc.exe) is required");
    assert!(result.success(), "Windows resource compilation failed");
    println!("cargo:rustc-link-arg={}", out.join("app.res").display());
}

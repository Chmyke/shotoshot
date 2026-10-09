fn main() {
    // Icon, version and manifest resources exist only for the Windows binary.
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() != Ok("windows") {
        return;
    }
    let root = std::env::var("CARGO_MANIFEST_DIR").expect("Cargo provides the package directory");
    println!("cargo:rerun-if-changed=app.manifest");
    println!("cargo:rustc-link-arg-bin=shotoshot=/MANIFEST:EMBED");
    println!("cargo:rustc-link-arg-bin=shotoshot=/MANIFESTINPUT:{root}/app.manifest");
    println!("cargo:rerun-if-changed=assets/shotoshot.ico");
    println!("cargo:rerun-if-env-changed=WindowsSdkDir");
    println!("cargo:rerun-if-env-changed=WindowsSDKVersion");
    let output =
        std::path::PathBuf::from(std::env::var_os("OUT_DIR").expect("Cargo output directory"));
    let script = output.join("shotoshot.rc");
    let resource = output.join("shotoshot.res");
    let icon = std::path::Path::new(&root).join("assets/shotoshot.ico");
    // RC accepts forward slashes, including paths containing spaces.
    let icon = icon.display().to_string().replace('\\', "/");
    std::fs::write(&script, resource_script(&icon)).expect("Write the resource script");
    let status = std::process::Command::new(resource_compiler())
        .arg("/nologo")
        .arg("/fo")
        .arg(&resource)
        .arg(&script)
        .status()
        .expect("Run the Windows SDK resource compiler (rc.exe)");
    assert!(status.success(), "Windows icon resource compilation failed");
    println!("cargo:rustc-link-arg-bin=shotoshot={}", resource.display());
}

/// Icon plus version metadata: an anonymous binary that hooks the keyboard and
/// captures the screen looks like spyware to antivirus heuristics.
fn resource_script(icon: &str) -> String {
    let part = |name| std::env::var(name).expect("Cargo provides the package version");
    let (major, minor, patch) = (
        part("CARGO_PKG_VERSION_MAJOR"),
        part("CARGO_PKG_VERSION_MINOR"),
        part("CARGO_PKG_VERSION_PATCH"),
    );
    let version = format!("{major}.{minor}.{patch}.0");
    let numeric = format!("{major},{minor},{patch},0");
    format!(
        r#"#pragma code_page(65001)
1 ICON "{icon}"
1 VERSIONINFO
FILEVERSION {numeric}
PRODUCTVERSION {numeric}
FILEOS 0x40004
FILETYPE 0x1
BEGIN
  BLOCK "StringFileInfo"
  BEGIN
    BLOCK "040904B0"
    BEGIN
      VALUE "CompanyName", "Mickaël Mancini"
      VALUE "FileDescription", "Shotoshot screenshot tool"
      VALUE "FileVersion", "{version}"
      VALUE "InternalName", "shotoshot"
      VALUE "LegalCopyright", "Copyright (C) 2026 Mickaël Mancini"
      VALUE "OriginalFilename", "shotoshot.exe"
      VALUE "ProductName", "Shotoshot"
      VALUE "ProductVersion", "{version}"
    END
  END
  BLOCK "VarFileInfo"
  BEGIN
    VALUE "Translation", 0x409, 1200
  END
END
"#
    )
}

fn resource_compiler() -> std::path::PathBuf {
    // Developer shells expose rc.exe directly; normal Cargo shells still have the SDK.
    if let Some(path) = std::env::var_os("PATH") {
        for directory in std::env::split_paths(&path) {
            let compiler = directory.join("rc.exe");
            if compiler.is_file() {
                return compiler;
            }
        }
    }
    let sdk = std::env::var_os("WindowsSdkDir")
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|| {
            std::path::PathBuf::from(
                std::env::var_os("ProgramFiles(x86)").expect("Windows Program Files directory"),
            )
            .join("Windows Kits/10")
        });
    let host = if cfg!(target_arch = "aarch64") {
        "arm64"
    } else {
        "x64"
    };
    let mut versions: Vec<_> = std::fs::read_dir(sdk.join("bin"))
        .expect("Locate the installed Windows SDK")
        .filter_map(Result::ok)
        .map(|entry| entry.path().join(host).join("rc.exe"))
        .filter(|path| path.is_file())
        .collect();
    versions.sort();
    versions
        .pop()
        .expect("Windows SDK rc.exe is required to embed the application icon")
}

fn main() {
    // tauri-build only embeds the Windows app manifest into binary targets
    // (`rustc-link-arg-bins`), so `#[tauri::test]` binaries would bind to
    // comctl32 v5 and fail at load with 0xC0000139 (tauri-apps/tauri#13419).
    // Workaround (tauri maintainer): disable the default manifest and embed it
    // via `cargo:rustc-link-arg`, which applies to every artifact (bins + tests).
    #[cfg(windows)]
    {
        let attributes = tauri_build::Attributes::new().windows_attributes(
            tauri_build::WindowsAttributes::new_without_app_manifest(),
        );
        tauri_build::try_build(attributes).expect("failed to run tauri-build");
        add_manifest();
    }
    #[cfg(not(windows))]
    tauri_build::build();
}

#[cfg(windows)]
fn add_manifest() {
    static WINDOWS_MANIFEST_FILE: &str = "windows-app-manifest.xml";

    let manifest = std::env::current_dir()
        .unwrap()
        .join(WINDOWS_MANIFEST_FILE);

    println!("cargo:rerun-if-changed={}", manifest.display());
    println!("cargo:rustc-link-arg=/MANIFEST:EMBED");
    println!(
        "cargo:rustc-link-arg=/MANIFESTINPUT:{}",
        manifest.to_str().unwrap()
    );
}

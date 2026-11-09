fn main() {
    // Le manifeste Windows (Common Controls v6) est embarqué par l'éditeur de liens dans tous les
    // exécutables, tests compris, et non par tauri-build (qui ne le met que dans le binaire) :
    // sans lui, un test qui construit une `App` Tauri ne démarre pas
    // (STATUS_ENTRYPOINT_NOT_FOUND : `TaskDialogIndirect` absent de comctl32 v5).
    let windows = tauri_build::WindowsAttributes::new_without_app_manifest();
    tauri_build::try_build(tauri_build::Attributes::new().windows_attributes(windows))
        .expect("échec de tauri-build");

    let target_os = std::env::var("CARGO_CFG_TARGET_OS").unwrap_or_default();
    let target_env = std::env::var("CARGO_CFG_TARGET_ENV").unwrap_or_default();
    if target_os == "windows" && target_env == "msvc" {
        let dir = std::env::var("CARGO_MANIFEST_DIR").expect("CARGO_MANIFEST_DIR");
        let manifest = std::path::Path::new(&dir).join("windows-app-manifest.xml");
        println!("cargo:rerun-if-changed={}", manifest.display());
        println!("cargo:rustc-link-arg=/MANIFEST:EMBED");
        println!("cargo:rustc-link-arg=/MANIFESTINPUT:{}", manifest.display());
    }
}

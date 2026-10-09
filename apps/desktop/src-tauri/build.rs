fn main() {
    if let Err(error) = tauri_build::try_build(
        tauri_build::Attributes::new()
            .app_manifest(tauri_build::AppManifest::new().commands(&["dispatch"])),
    ) {
        eprintln!("Desktop build configuration failed: {error}");
        std::process::exit(1);
    }
}

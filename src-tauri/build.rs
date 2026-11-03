fn main() {
    tauri_build::try_build(
        tauri_build::Attributes::new()
            .app_manifest(tauri_build::AppManifest::new().commands(&["dock_items"])),
    )
    .expect("the app manifest lists only commands that exist");
}

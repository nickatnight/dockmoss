fn main() {
    #[expect(
        clippy::expect_used,
        reason = "a build script reports failure by panicking"
    )]
    tauri_build::try_build(
        tauri_build::Attributes::new().app_manifest(
            tauri_build::AppManifest::new().commands(&["watch_dock", "watch_growth"]),
        ),
    )
    .expect("the app manifest lists only commands that exist");
}

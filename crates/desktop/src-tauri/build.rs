fn main() {
    tauri_build::try_build(
        tauri_build::Attributes::new().app_manifest(tauri_build::AppManifest::new().commands(&[
            "install_update",
            "check_updates_now",
            "pending_update",
            "refresh_tray",
            "window_minimize",
            "window_toggle_maximize",
            "window_is_maximized",
            "window_close",
        ])),
    )
    .expect("failed to run tauri-build");
}

use tauri::AppHandle;
use tauri::menu::{Menu, MenuItem};
use tauri::tray::TrayIconBuilder;

const QUIT_ID: &str = "quit";

pub fn build(app: &AppHandle) -> Result<(), crate::Error> {
    let quit = MenuItem::with_id(app, QUIT_ID, "Quit Dock Moss", true, Some("CmdOrCtrl+Q"))?;
    let menu = Menu::with_items(app, &[&quit])?;

    TrayIconBuilder::with_id("main")
        .icon(tauri::include_image!("icons/tray.png"))
        .icon_as_template(true)
        .tooltip("Dock Moss")
        .menu(&menu)
        .on_menu_event(|app, event| {
            if event.id() == QUIT_ID {
                tracing::info!("quit requested from tray");
                app.exit(0);
            }
        })
        .build(app)?;

    Ok(())
}

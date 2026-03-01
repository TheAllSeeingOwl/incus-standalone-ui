mod commands;
mod config;
mod menu;
mod proxy;

use tauri::{Manager, WebviewUrl, WebviewWindowBuilder};
use tauri_plugin_store::StoreExt;

pub struct ProxyPort(pub u16);

const DEFAULT_SOCKET: &str = "/var/lib/incus/unix.socket";

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tracing_subscriber::fmt::init();

    tauri::Builder::default()
        .plugin(tauri_plugin_single_instance::init(|app, _args, _cwd| {
            // A second instance was launched — focus the existing window.
            if let Some(win) = app.get_webview_window("main") {
                let _ = win.show();
                let _ = win.set_focus();
            }
        }))
        .plugin(tauri_plugin_store::Builder::default().build())
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_opener::init())
        .setup(|app| {
            let store = app.store("incus-settings.json")?;
            let mut cfg = config::load_config(&store);
            let is_first_run = store.get("host").is_none() && store.get("socketPath").is_none();

            // Auto-detect Unix socket on first run
            if is_first_run && std::path::Path::new(DEFAULT_SOCKET).exists() {
                cfg.socket_path = Some(DEFAULT_SOCKET.into());
                let _ = config::save_config(&store, &cfg);
            }

            drop(store);

            let (proxy_port, proxy_state) =
                tauri::async_runtime::block_on(proxy::start_proxy(cfg))
                    .expect("failed to start proxy");

            app.manage(proxy_state);
            app.manage(ProxyPort(proxy_port));
            app.manage(commands::FirstRun(is_first_run));

            let main_win = WebviewWindowBuilder::new(
                app,
                "main",
                WebviewUrl::App("index.html".into()),
            )
            .title("Incus")
            .inner_size(1280.0, 860.0)
            .visible(true)
            .decorations(cfg!(feature = "native-titlebar"))
            .zoom_hotkeys_enabled(true)
            .build()?;

            main_win.on_window_event({
                let win = main_win.clone();
                move |event| {
                    if let tauri::WindowEvent::CloseRequested { api, .. } = event {
                        api.prevent_close();
                        let _ = win.hide();
                    }
                }
            });

            menu::setup_tray(app.handle())?;
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::get_proxy_port,
            commands::get_build_info,
            commands::get_settings,
            commands::save_settings,
            commands::reload_main_window,
            commands::open_external_url,
            commands::open_docs_window,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}

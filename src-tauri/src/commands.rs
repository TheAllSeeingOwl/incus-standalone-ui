use std::sync::Arc;

use tauri::{AppHandle, Manager, State, WebviewUrl, WebviewWindowBuilder};
use tauri_plugin_store::StoreExt;

use crate::{
    config::{self, AppConfig},
    proxy::{build_client, ProxyState, ProxyStateInner},
    ProxyPort,
};

pub struct FirstRun(pub bool);

#[derive(serde::Serialize)]
pub struct BuildInfo {
    pub incus_version: &'static str,
    pub incus_commit: &'static str,
    pub ui_commit: &'static str,
}

#[tauri::command]
pub fn get_build_info() -> BuildInfo {
    BuildInfo {
        incus_version: env!("INCUS_VERSION"),
        incus_commit: env!("INCUS_COMMIT"),
        ui_commit: env!("INCUS_UI_COMMIT"),
    }
}

#[derive(serde::Serialize)]
pub struct ProxyInfo {
    pub port: u16,
    pub first_run: bool,
}

#[tauri::command]
pub fn get_proxy_port(
    proxy_port: State<'_, ProxyPort>,
    first_run: State<'_, FirstRun>,
) -> ProxyInfo {
    ProxyInfo {
        port: proxy_port.inner().0,
        first_run: first_run.inner().0,
    }
}

#[tauri::command]
pub async fn get_settings(app: AppHandle) -> Result<AppConfig, String> {
    let store = app.store("incus-settings.json").map_err(|e| e.to_string())?;
    Ok(config::load_config(&store))
}

#[tauri::command]
pub async fn save_settings(
    app: AppHandle,
    proxy_state: State<'_, ProxyState>,
    config: AppConfig,
) -> Result<(), String> {
    let store = app.store("incus-settings.json").map_err(|e| e.to_string())?;
    config::save_config(&store, &config).map_err(|e| e.to_string())?;

    let client = if config.socket_path.is_some() {
        None
    } else {
        Some(build_client(&config).map_err(|e| e.to_string())?)
    };

    proxy_state.store(Arc::new(ProxyStateInner { config, client }));

    Ok(())
}

/// Open an external URL (e.g. GitHub discussions, bug reports) in the system browser.
#[tauri::command]
pub async fn open_external_url(url: String) -> Result<(), String> {
    // Only allow http/https to avoid arbitrary URI scheme abuse
    if !url.starts_with("https://") && !url.starts_with("http://") {
        return Err("only http/https URLs are allowed".into());
    }
    tauri_plugin_opener::open_url(&url, None::<&str>).map_err(|e| e.to_string())
}

/// Open a documentation URL in a dedicated in-app window.
/// Only accepts URLs pointing to our local /docs/ path.
#[tauri::command]
pub async fn open_docs_window(
    app: AppHandle,
    proxy_port: State<'_, ProxyPort>,
    url: String,
) -> Result<(), String> {
    let parsed = url.parse::<tauri::Url>().map_err(|e| e.to_string())?;

    let ok = parsed.host_str() == Some("127.0.0.1")
        && parsed.port() == Some(proxy_port.inner().0)
        && parsed.path().starts_with("/docs");
    if !ok {
        return Err("docs URL must point to the local proxy /docs/ path".into());
    }

    if let Some(win) = app.get_webview_window("docs") {
        let _ = win.eval(&format!("window.location.href = {:?}", url));
        let _ = win.show();
        let _ = win.set_focus();
        return Ok(());
    }

    WebviewWindowBuilder::new(&app, "docs", WebviewUrl::External(parsed))
        .title("Incus Documentation")
        .inner_size(1100.0, 800.0)
        .zoom_hotkeys_enabled(true)
        .build()
        .map_err(|e| e.to_string())?;

    Ok(())
}

#[tauri::command]
pub async fn reload_main_window(app: AppHandle) -> Result<(), String> {
    if let Some(win) = app.get_webview_window("main") {
        win.eval("window.__reloadIncus && window.__reloadIncus()")
            .map_err(|e| e.to_string())?;
        let _ = win.show();
        let _ = win.set_focus();
    }
    Ok(())
}

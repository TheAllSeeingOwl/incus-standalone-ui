use serde::{Deserialize, Serialize};

const DEFAULT_SOCKET: &str = "/var/lib/incus/unix.socket";

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AppConfig {
    pub host: String,
    pub port: u16,
    pub accept_invalid_certs: bool,
    pub ca_cert_path: Option<String>,
    pub client_cert_path: Option<String>,
    pub client_key_path: Option<String>,
    pub socket_path: Option<String>,
}

impl Default for AppConfig {
    fn default() -> Self {
        let socket_path = if std::path::Path::new(DEFAULT_SOCKET).exists() {
            Some(DEFAULT_SOCKET.into())
        } else {
            None
        };
        Self {
            host: "localhost".into(),
            port: 8443,
            accept_invalid_certs: false,
            ca_cert_path: None,
            client_cert_path: None,
            client_key_path: None,
            socket_path,
        }
    }
}

pub fn load_config(store: &tauri_plugin_store::Store<tauri::Wry>) -> AppConfig {
    let has_any_key = store.get("host").is_some() || store.get("socketPath").is_some();

    if !has_any_key {
        return AppConfig::default();
    }

    AppConfig {
        host: store
            .get("host")
            .and_then(|v| v.as_str().map(String::from))
            .unwrap_or_else(|| "localhost".into()),
        port: store
            .get("port")
            .and_then(|v| v.as_u64().map(|n| n as u16))
            .unwrap_or(8443),
        accept_invalid_certs: store
            .get("acceptInvalidCerts")
            .and_then(|v| v.as_bool())
            .unwrap_or(false),
        ca_cert_path: store
            .get("caCertPath")
            .and_then(|v| v.as_str().map(String::from)),
        client_cert_path: store
            .get("clientCertPath")
            .and_then(|v| v.as_str().map(String::from)),
        client_key_path: store
            .get("clientKeyPath")
            .and_then(|v| v.as_str().map(String::from)),
        socket_path: store
            .get("socketPath")
            .and_then(|v| v.as_str().map(String::from)),
    }
}

pub fn save_config(
    store: &tauri_plugin_store::Store<tauri::Wry>,
    config: &AppConfig,
) -> anyhow::Result<()> {
    store.set("host", serde_json::json!(config.host));
    store.set("port", serde_json::json!(config.port));
    store.set("acceptInvalidCerts", serde_json::json!(config.accept_invalid_certs));
    store.set("caCertPath", serde_json::json!(config.ca_cert_path));
    store.set("clientCertPath", serde_json::json!(config.client_cert_path));
    store.set("clientKeyPath", serde_json::json!(config.client_key_path));
    store.set("socketPath", serde_json::json!(config.socket_path));
    store.save()?;
    Ok(())
}

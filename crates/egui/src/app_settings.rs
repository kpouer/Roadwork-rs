use roadwork_core::model::opendata_data::OpendataData;
use roadwork_core::settings::Settings;
use roadwork_service::SyncConfig;
use std::collections::HashMap;

#[cfg(target_arch = "wasm32")]
const SETTINGS_KEY: &str = "roadwork-settings";
#[cfg(not(target_arch = "wasm32"))]
const SETTINGS_FILE: &str = "settings.json";

#[cfg(target_arch = "wasm32")]
const OPENDATA_CACHE_KEY: &str = "roadwork-opendata-cache";
#[cfg(not(target_arch = "wasm32"))]
const OPENDATA_CACHE_FILE: &str = "opendata_cache.json";

pub fn load_settings() -> Settings {
    #[cfg(target_arch = "wasm32")]
    {
        if let Some(storage) = web_sys::window().and_then(|w| w.local_storage().ok().flatten()) {
            if let Ok(Some(json)) = storage.get_item(SETTINGS_KEY)
                && let Ok(settings) = serde_json::from_str::<Settings>(&json)
            {
                return settings;
            }
        }
    }
    #[cfg(not(target_arch = "wasm32"))]
    {
        if let Ok(json) = std::fs::read_to_string(SETTINGS_FILE)
            && let Ok(settings) = serde_json::from_str::<Settings>(&json)
        {
            return settings;
        }
    }
    Settings::default()
}

pub fn save_settings(settings: &Settings) {
    let Ok(json) = serde_json::to_string(settings) else {
        return;
    };
    #[cfg(target_arch = "wasm32")]
    if let Some(storage) = web_sys::window().and_then(|w| w.local_storage().ok().flatten()) {
        let _ = storage.set_item(SETTINGS_KEY, &json);
    }
    #[cfg(not(target_arch = "wasm32"))]
    let _ = std::fs::write(SETTINGS_FILE, json);
}

pub fn sync_config(settings: &Settings) -> Option<SyncConfig> {
    if !settings.synchronization_enabled || settings.synchronization_url.is_empty() {
        return None;
    }
    Some(SyncConfig {
        url: settings.synchronization_url.clone(),
        team: settings.synchronization_team.clone(),
        login: settings.synchronization_login.clone(),
        password: settings.synchronization_password.clone(),
        enabled: true,
    })
}

pub fn load_opendata_cache() -> HashMap<String, OpendataData> {
    #[cfg(target_arch = "wasm32")]
    {
        if let Some(storage) = web_sys::window().and_then(|w| w.local_storage().ok().flatten()) {
            if let Ok(Some(json)) = storage.get_item(OPENDATA_CACHE_KEY)
                && let Ok(cache) = serde_json::from_str::<HashMap<String, OpendataData>>(&json)
            {
                return cache;
            }
        }
    }
    #[cfg(not(target_arch = "wasm32"))]
    {
        if let Ok(json) = std::fs::read_to_string(OPENDATA_CACHE_FILE)
            && let Ok(cache) = serde_json::from_str::<HashMap<String, OpendataData>>(&json)
        {
            return cache;
        }
    }
    HashMap::new()
}

pub fn save_opendata_cache(name: &str, data: &OpendataData) {
    let mut cache = load_opendata_cache();
    cache.insert(name.to_string(), data.clone());
    let Ok(json) = serde_json::to_string(&cache) else {
        return;
    };
    #[cfg(target_arch = "wasm32")]
    if let Some(storage) = web_sys::window().and_then(|w| w.local_storage().ok().flatten()) {
        let _ = storage.set_item(OPENDATA_CACHE_KEY, &json);
    }
    #[cfg(not(target_arch = "wasm32"))]
    let _ = std::fs::write(OPENDATA_CACHE_FILE, json);
}

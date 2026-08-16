use roadwork_core::settings::Settings;
use roadwork_service::SyncConfig;

const SETTINGS_KEY: &str = "roadwork-settings";

pub fn load_settings() -> Settings {
    if let Some(storage) = web_sys::window().and_then(|w| w.local_storage().ok().flatten())
        && let Ok(Some(json)) = storage.get_item(SETTINGS_KEY)
        && let Ok(settings) = serde_json::from_str::<Settings>(&json)
    {
        return settings;
    }
    Settings::default()
}

pub fn save_settings(settings: &Settings) {
    let Ok(json) = serde_json::to_string(settings) else {
        return;
    };
    store_in_cache(SETTINGS_KEY, &json);
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

fn store_in_cache(key: &str, json: &str) {
    if let Some(storage) = web_sys::window().and_then(|w| w.local_storage().ok().flatten()) {
        let _ = storage.set_item(key, &json);
    }
}

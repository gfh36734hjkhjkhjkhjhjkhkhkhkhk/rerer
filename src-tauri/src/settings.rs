use std::fs;
use std::path::Path;

use serde::{Deserialize, Serialize};

#[derive(Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct Settings {
    pub strategy: String,
    pub engine_on: bool,
    pub games: bool,
    pub ai: bool,
    pub tg_on: bool,
    pub tg_port: u16,
    pub tg_secret: String,
    pub autostart: bool,
    pub tray: bool,
    pub auto_update: bool,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            strategy: "general.bat".into(),
            engine_on: false,
            games: false,
            ai: false,
            tg_on: false,
            tg_port: 1443,
            tg_secret: String::new(),
            autostart: false,
            tray: true,
            auto_update: true,
        }
    }
}

impl Settings {
    pub fn load(path: &Path) -> Self {
        fs::read_to_string(path)
            .ok()
            .and_then(|t| serde_json::from_str(&t).ok())
            .unwrap_or_default()
    }

    /// Сохранить через временный файл: если Prism закроется посреди записи,
    /// старые настройки (и секрет прокси, уже добавленного в Telegram) не пропадут.
    pub fn save(&self, path: &Path) {
        if let Some(dir) = path.parent() {
            let _ = fs::create_dir_all(dir);
        }
        let Ok(text) = serde_json::to_string_pretty(self) else { return };
        let tmp = path.with_extension("json.tmp");
        if fs::write(&tmp, text).is_ok() && fs::rename(&tmp, path).is_err() {
            let _ = fs::remove_file(&tmp);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn saves_and_loads() {
        let dir = std::env::temp_dir().join(format!("prism-settings-test-{}", std::process::id()));
        let path = dir.join("settings.json");
        let s = Settings { tg_secret: "ab".repeat(16), games: true, ..Settings::default() };
        s.save(&path);
        // Повторное сохранение заменяет файл, временный файл не остаётся.
        s.save(&path);
        let back = Settings::load(&path);
        assert_eq!(back.tg_secret, s.tg_secret);
        assert!(back.games);
        assert!(!path.with_extension("json.tmp").exists());
        let _ = fs::remove_dir_all(&dir);
    }
}

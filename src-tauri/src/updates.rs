//! Обновления: сама программа Prism — из релизов lucudar/prism,
//! обходы zapret — из официальных релизов Flowseal/zapret-discord-youtube.
//! Каждый скачанный файл сверяется с SHA-256, который публикует GitHub.

use std::fs;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::time::Duration;

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

pub const APP_REPO: &str = "lucudar/prism";
pub const ZAPRET_REPO: &str = "Flowseal/zapret-discord-youtube";

#[derive(Deserialize, Clone)]
pub struct Asset {
    pub name: String,
    pub browser_download_url: String,
    #[serde(default)]
    pub digest: Option<String>,
}

#[derive(Deserialize, Clone)]
pub struct Release {
    pub tag_name: String,
    #[serde(default)]
    pub html_url: String,
    #[serde(default)]
    pub assets: Vec<Asset>,
}

#[derive(Serialize, Clone, Default)]
pub struct Item {
    pub current: String,
    pub latest: String,
    pub available: bool,
}

#[derive(Serialize, Clone, Default)]
pub struct Info {
    pub app: Item,
    pub zapret: Item,
}

/// «v1.10.3», «flowseal-1.10.3» → [1, 10, 3]
pub fn parse_version(s: &str) -> Vec<u32> {
    s.split(|c: char| !c.is_ascii_digit()).filter(|p| !p.is_empty()).filter_map(|p| p.parse().ok()).collect()
}

pub fn newer(latest: &str, current: &str) -> bool {
    let (a, b) = (parse_version(latest), parse_version(current));
    !a.is_empty() && a > b
}

fn client(secs: u64) -> Result<reqwest::Client, String> {
    reqwest::Client::builder()
        .user_agent(concat!("Prism/", env!("CARGO_PKG_VERSION")))
        .timeout(Duration::from_secs(secs))
        .build()
        .map_err(|e| e.to_string())
}

pub async fn latest(repo: &str) -> Result<Release, String> {
    client(20)?
        .get(format!("https://api.github.com/repos/{repo}/releases/latest"))
        .header("Accept", "application/vnd.github+json")
        .send()
        .await
        .and_then(|r| r.error_for_status())
        .map_err(|e| format!("Не удалось проверить обновления: {e}"))?
        .json::<Release>()
        .await
        .map_err(|e| format!("Не удалось прочитать ответ GitHub: {e}"))
}

/// Скачать файл и убедиться, что SHA-256 совпадает с тем, что опубликовал GitHub.
pub async fn download_verified(asset: &Asset) -> Result<Vec<u8>, String> {
    let expected = asset
        .digest
        .as_deref()
        .and_then(|d| d.strip_prefix("sha256:"))
        .ok_or("GitHub не сообщил хэш файла, обновление отменено ради безопасности")?
        .to_lowercase();
    let bytes = client(600)?
        .get(&asset.browser_download_url)
        .send()
        .await
        .and_then(|r| r.error_for_status())
        .map_err(|e| format!("Не удалось скачать обновление: {e}"))?
        .bytes()
        .await
        .map_err(|e| format!("Загрузка прервалась: {e}"))?;
    let got: String = Sha256::digest(&bytes).iter().map(|b| format!("{b:02x}")).collect();
    if got != expected {
        return Err("Хэш скачанного файла не совпал, обновление отменено".into());
    }
    Ok(bytes.to_vec())
}

pub fn zapret_asset(r: &Release) -> Option<&Asset> {
    r.assets.iter().find(|a| a.name.starts_with("zapret-discord-youtube") && a.name.ends_with(".zip"))
}

pub fn app_asset(r: &Release) -> Option<&Asset> {
    r.assets.iter().find(|a| a.name.ends_with("-setup.exe"))
}

/// Распаковать архив Flowseal в папку в нашем формате: bin/, lists/, strategies/.
pub fn unpack_zapret(zip_bytes: &[u8], dst: &Path, version: &str) -> Result<(), String> {
    let mut zip = zip::ZipArchive::new(std::io::Cursor::new(zip_bytes)).map_err(|e| format!("Архив повреждён: {e}"))?;
    if dst.exists() {
        fs::remove_dir_all(dst).map_err(|e| e.to_string())?;
    }
    let mut strategies = 0;
    let mut has_winws = false;
    for i in 0..zip.len() {
        let mut f = zip.by_index(i).map_err(|e| e.to_string())?;
        if f.is_dir() {
            continue;
        }
        let Some(path) = f.enclosed_name() else { continue };
        // Убираем верхнюю папку «zapret-discord-youtube-x.y.z/», если она есть.
        let parts: Vec<String> = path.components().map(|c| c.as_os_str().to_string_lossy().to_string()).collect();
        let parts = if parts.len() > 1 && !["bin", "lists"].contains(&parts[0].as_str()) && !parts[0].to_lowercase().ends_with(".bat") {
            parts[1..].to_vec()
        } else {
            parts
        };
        let rel: Option<PathBuf> = match parts.as_slice() {
            [dir, rest @ ..] if (dir == "bin" || dir == "lists") && !rest.is_empty() => {
                Some(rest.iter().fold(PathBuf::from(dir), |p, s| p.join(s)))
            }
            [file] if file.to_lowercase().starts_with("general") && file.to_lowercase().ends_with(".bat") => {
                strategies += 1;
                Some(PathBuf::from("strategies").join(file))
            }
            _ => None,
        };
        let Some(rel) = rel else { continue };
        if rel.ends_with("winws.exe") {
            has_winws = true;
        }
        let out = dst.join(&rel);
        if let Some(parent) = out.parent() {
            fs::create_dir_all(parent).map_err(|e| e.to_string())?;
        }
        let mut buf = Vec::new();
        f.read_to_end(&mut buf).map_err(|e| e.to_string())?;
        fs::write(&out, buf).map_err(|e| e.to_string())?;
    }
    if !has_winws || strategies == 0 {
        return Err("В новой версии не найдены winws.exe или стратегии, обновление отменено".into());
    }
    fs::write(dst.join("version.txt"), format!("flowseal-{}", version.trim_start_matches('v'))).map_err(|e| e.to_string())?;
    Ok(())
}

/// Перенести пользовательские списки из старой папки в новую.
pub fn carry_user_lists(old: &Path, new: &Path) {
    if let Ok(dir) = fs::read_dir(old.join("lists")) {
        for e in dir.flatten() {
            let name = e.file_name().to_string_lossy().to_string();
            if name.ends_with("-user.txt") {
                let _ = fs::copy(e.path(), new.join("lists").join(&name));
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn versions() {
        assert!(newer("v1.10.4", "flowseal-1.10.3"));
        assert!(newer("1.11", "1.10.9"));
        assert!(!newer("v0.1.0", "0.1.0"));
        assert!(!newer("", "0.1.0"));
    }
}

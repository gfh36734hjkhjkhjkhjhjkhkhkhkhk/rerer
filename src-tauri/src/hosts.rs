//! Доступ к нейросетям: адреса из публичного списка ImMALWARE/dns.malw.link
//! записываются в файл hosts Windows. Берём только разделы с нейросетями.
//!
//! Файл hosts читается и пишется байтами: в нём бывают комментарии в cp1251 или cp866,
//! а иногда его держит антивирус. В обоих случаях чужие записи должны остаться как были.

use std::fs;
use std::io::ErrorKind;
use std::path::{Path, PathBuf};

use crate::util::run_quiet;

const URL: &str = "https://raw.githubusercontent.com/ImMALWARE/dns.malw.link/master/hosts";
const START: &str = "# >>> Prism: нейросети (не редактируйте этот блок)";
const END: &str = "# <<< Prism";
const SECTIONS: &[&str] = &["OpenAI", "Microsoft", "GitHub Copilot", "ElevenLabs", "Grok", "DeepL", "Google", "Claude"];

pub fn hosts_path() -> PathBuf {
    let root = std::env::var("SystemRoot").unwrap_or_else(|_| "C:\\Windows".into());
    Path::new(&root).join("System32").join("drivers").join("etc").join("hosts")
}

/// Оставить из списка только нужные разделы и строки «IP домен».
pub fn filter_list(raw: &str) -> String {
    let mut out = Vec::new();
    let mut take = false;
    for line in raw.lines() {
        let t = line.trim();
        if let Some(name) = t.strip_prefix("# ") {
            if !t.starts_with("###") {
                take = SECTIONS.iter().any(|s| s.eq_ignore_ascii_case(name.trim()));
                if take {
                    out.push(format!("# {}", name.trim()));
                }
            }
            continue;
        }
        if !take || t.is_empty() || t.starts_with('#') || t.starts_with("0.0.0.0") {
            continue;
        }
        let mut it = t.split_whitespace();
        if let (Some(ip), Some(host), None) = (it.next(), it.next(), it.next()) {
            if ip.parse::<std::net::IpAddr>().is_ok() && host.contains('.') {
                out.push(format!("{ip} {host}"));
            }
        }
    }
    out.join("\r\n")
}

fn contains(hay: &[u8], needle: &[u8]) -> bool {
    hay.windows(needle.len()).any(|w| w == needle)
}

fn split_lines(text: &[u8]) -> Vec<&[u8]> {
    text.split(|&b| b == b'\n').map(|l| l.strip_suffix(b"\r").unwrap_or(l)).collect()
}

fn is_marker(line: &[u8], marker: &str) -> bool {
    line.trim_ascii() == marker.as_bytes()
}

/// Номера строк блока Prism (включая заголовок и конец).
fn block_lines(lines: &[&[u8]]) -> Vec<bool> {
    let mut ours = vec![false; lines.len()];
    let mut i = 0;
    while i < lines.len() {
        if is_marker(lines[i], START) {
            match (i + 1..lines.len()).find(|&j| is_marker(lines[j], END)) {
                Some(end) => {
                    ours[i..=end].iter_mut().for_each(|x| *x = true);
                    i = end;
                }
                // Конец блока стёрли вручную: убираем только заголовок,
                // чтобы не задеть чужие записи после него.
                None => ours[i] = true,
            }
        }
        i += 1;
    }
    ours
}

/// Убрать блок Prism. Остальные строки сохраняются байт в байт.
fn strip_block(text: &[u8]) -> Vec<u8> {
    let lines = split_lines(text);
    let ours = block_lines(&lines);
    let mut out: Vec<&[u8]> = lines.iter().zip(&ours).filter(|(_, o)| !**o).map(|(l, _)| *l).collect();
    while out.last().is_some_and(|l| l.trim_ascii().is_empty()) {
        out.pop();
    }
    out.join(&b"\r\n"[..])
}

fn check_encoding(text: &[u8]) -> Result<(), String> {
    if text.starts_with(&[0xFF, 0xFE]) || text.starts_with(&[0xFE, 0xFF]) {
        return Err("Файл hosts сохранён в кодировке UTF-16, Prism не будет его менять. Пересохраните его в UTF-8.".into());
    }
    Ok(())
}

/// Содержимое hosts с новым блоком Prism в конце.
fn with_block(current: &[u8], list: &str) -> Result<Vec<u8>, String> {
    check_encoding(current)?;
    let mut body = strip_block(current);
    if !body.is_empty() {
        body.extend_from_slice(b"\r\n\r\n");
    }
    body.extend_from_slice(format!("{START}\r\n{list}\r\n{END}\r\n").as_bytes());
    Ok(body)
}

/// Содержимое hosts без блока Prism или None, если блока нет.
fn without_block(current: &[u8]) -> Result<Option<Vec<u8>>, String> {
    if !contains(current, START.as_bytes()) {
        return Ok(None);
    }
    check_encoding(current)?;
    let mut body = strip_block(current);
    body.extend_from_slice(b"\r\n");
    Ok(Some(body))
}

/// Активные записи для YouTube вне блока Prism: из-за них YouTube может не открываться.
fn youtube_lines(text: &[u8]) -> Vec<String> {
    let lines = split_lines(text);
    let ours = block_lines(&lines);
    lines
        .iter()
        .zip(&ours)
        .filter(|(_, o)| !**o)
        .map(|(l, _)| String::from_utf8_lossy(l).trim().to_string())
        .filter(|l| !l.starts_with('#'))
        .filter(|l| {
            let l = l.to_lowercase();
            l.split_whitespace().skip(1).any(|h| {
                let h = h.split('#').next().unwrap_or("");
                ["youtube.com", "youtu.be", "googlevideo.com"].iter().any(|d| h == *d || h.ends_with(&format!(".{d}")))
            })
        })
        .collect()
}

fn read_hosts() -> Result<Vec<u8>, String> {
    match fs::read(hosts_path()) {
        Ok(b) => Ok(b),
        Err(e) if e.kind() == ErrorKind::NotFound => Ok(Vec::new()),
        // Продолжать с пустым содержимым нельзя: так затёрлись бы чужие записи.
        Err(e) => Err(format!("Не удалось прочитать файл hosts: {e}. Возможно, его блокирует антивирус.")),
    }
}

fn write_hosts(body: &[u8]) -> Result<(), String> {
    let p = hosts_path();
    if let Ok(meta) = fs::metadata(&p) {
        let mut perm = meta.permissions();
        if perm.readonly() {
            #[allow(clippy::permissions_set_readonly_false)]
            perm.set_readonly(false);
            let _ = fs::set_permissions(&p, perm);
        }
    }
    fs::write(&p, body).map_err(|e| {
        format!("Не удалось изменить файл hosts: {e}. Возможно, его блокирует антивирус.")
    })?;
    run_quiet("ipconfig", &["/flushdns"]);
    Ok(())
}

pub async fn download() -> Result<String, String> {
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(15))
        .build()
        .map_err(|e| e.to_string())?;
    let text = client
        .get(URL)
        .send()
        .await
        .and_then(|r| r.error_for_status())
        .map_err(|e| format!("Не удалось скачать список адресов: {e}"))?
        .text()
        .await
        .map_err(|e| e.to_string())?;
    let list = filter_list(&text);
    if list.lines().filter(|l| !l.starts_with('#')).count() < 5 {
        return Err("Список адресов пришёл пустым, попробуйте позже".into());
    }
    Ok(list)
}

pub fn enable(list: &str) -> Result<(), String> {
    let body = with_block(&read_hosts()?, list)?;
    write_hosts(&body)
}

pub fn disable() -> Result<(), String> {
    match without_block(&read_hosts()?)? {
        Some(body) => write_hosts(&body),
        None => Ok(()),
    }
}

/// Записан ли сейчас блок Prism в hosts.
pub fn is_enabled() -> bool {
    fs::read(hosts_path()).map(|t| contains(&t, START.as_bytes())).unwrap_or(false)
}

/// Записи для YouTube в hosts, которые добавил не Prism.
pub fn youtube_entries() -> Vec<String> {
    fs::read(hosts_path()).map(|t| youtube_lines(&t)).unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn filters_sections() {
        let raw = "### dns.malw.link: hosts file\n# OpenAI\n1.2.3.4 chatgpt.com\n# Spotify\n5.6.7.8 spotify.com\n# Блокировка\n0.0.0.0 ads.com\n### end\n";
        let f = filter_list(raw);
        assert!(f.contains("1.2.3.4 chatgpt.com"));
        assert!(!f.contains("spotify"));
        assert!(!f.contains("ads.com"));
    }

    #[test]
    fn strips_block() {
        let t = format!("127.0.0.1 localhost\r\n\r\n{START}\r\n1.1.1.1 a.com\r\n{END}\r\n");
        assert_eq!(strip_block(t.as_bytes()), b"127.0.0.1 localhost");
    }

    #[test]
    fn keeps_lines_in_other_encodings() {
        // Комментарий «Привет» в cp1251: это не UTF-8, но строку нельзя потерять.
        let mut current = b"127.0.0.1 localhost\r\n# ".to_vec();
        current.extend_from_slice(&[0xCF, 0xF0, 0xE8, 0xE2, 0xE5, 0xF2]);
        current.extend_from_slice(b"\r\n10.0.0.1 nas.local\r\n");
        let on = with_block(&current, "1.1.1.1 chatgpt.com").unwrap();
        assert!(on.starts_with(&current[..current.len() - 2]));
        assert!(contains(&on, b"1.1.1.1 chatgpt.com"));
        let off = without_block(&on).unwrap().unwrap();
        assert_eq!(off, current);
        // Повторное включение не дублирует блок.
        let twice = with_block(&on, "1.1.1.1 chatgpt.com").unwrap();
        assert_eq!(twice, on);
    }

    #[test]
    fn lost_block_end_keeps_user_lines() {
        let t = format!("{START}\r\n1.1.1.1 a.com\r\n10.0.0.1 nas.local\r\n");
        let out = strip_block(t.as_bytes());
        assert_eq!(out, b"1.1.1.1 a.com\r\n10.0.0.1 nas.local");
    }

    #[test]
    fn refuses_utf16() {
        let t = [0xFF, 0xFE, b'#', 0, b'\n', 0];
        assert!(with_block(&t, "1.1.1.1 a.com").is_err());
        assert_eq!(without_block(&t).unwrap(), None);
    }

    #[test]
    fn nothing_to_remove() {
        assert_eq!(without_block(b"127.0.0.1 localhost\r\n").unwrap(), None);
    }

    #[test]
    fn finds_youtube_entries() {
        let t = format!(
            "# 1.2.3.4 youtube.com\r\n127.0.0.1 localhost\r\n1.2.3.4 www.youtube.com # old\r\n5.6.7.8 notyoutube.com\r\n{START}\r\n9.9.9.9 youtube.com\r\n{END}\r\n"
        );
        assert_eq!(youtube_lines(t.as_bytes()), vec!["1.2.3.4 www.youtube.com # old".to_string()]);
    }
}

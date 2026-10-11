//! Управление движком zapret (winws.exe из сборки Flowseal).
//! Стратегии берутся из оригинальных .bat-файлов Flowseal без изменений:
//! мы только достаём из них аргументы winws.exe и подставляем пути.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Child, Stdio};

use serde::Serialize;

use crate::util::{hidden, run_quiet, Log};

#[derive(Clone, Serialize)]
pub struct Strategy {
    pub id: String,
    pub name: String,
    pub desc: String,
}

#[derive(Clone)]
pub struct ZapretPaths {
    pub root: PathBuf,
}

impl ZapretPaths {
    pub fn bin(&self) -> PathBuf {
        self.root.join("bin")
    }
    pub fn lists(&self) -> PathBuf {
        self.root.join("lists")
    }
    pub fn strategies(&self) -> PathBuf {
        self.root.join("strategies")
    }
    pub fn user_list(&self) -> PathBuf {
        self.lists().join("list-general-user.txt")
    }
}

fn describe(name: &str) -> &'static str {
    let n = name.to_uppercase();
    if n == "ОСНОВНАЯ" {
        "Рекомендуемая, начните с неё"
    } else if n.contains("FAKE TLS") {
        "Поддельное TLS-приветствие, часто помогает на мобильном интернете"
    } else if n.contains("SIMPLE FAKE") {
        "Простые фейковые пакеты"
    } else if n.contains("EXP") {
        "Экспериментальная"
    } else {
        "Альтернативная разрезка пакетов"
    }
}

fn natural_key(s: &str) -> (String, u32) {
    let digits: String = s.chars().rev().take_while(|c| c.is_ascii_digit()).collect::<String>().chars().rev().collect();
    let prefix = s[..s.len() - digits.len()].to_string();
    (prefix, digits.parse().unwrap_or(0))
}

pub fn list_strategies(paths: &ZapretPaths) -> Vec<Strategy> {
    let mut out = Vec::new();
    if let Ok(dir) = fs::read_dir(paths.strategies()) {
        for e in dir.flatten() {
            let file = e.file_name().to_string_lossy().to_string();
            if !file.to_lowercase().ends_with(".bat") || !file.to_lowercase().starts_with("general") {
                continue;
            }
            let stem = file[..file.len() - 4].to_string();
            let name = match (stem.find('('), stem.rfind(')')) {
                (Some(a), Some(b)) if b > a => stem[a + 1..b].trim().to_string(),
                _ => "Основная".to_string(),
            };
            out.push(Strategy { desc: describe(&name).into(), id: file, name });
        }
    }
    out.sort_by(|a, b| {
        let ka = if a.name == "Основная" { (String::new(), 0) } else { natural_key(&a.name) };
        let kb = if b.name == "Основная" { (String::new(), 0) } else { natural_key(&b.name) };
        ka.cmp(&kb)
    });
    out
}

/// Разбить командную строку на аргументы с учётом кавычек.
fn tokenize(s: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut cur = String::new();
    let mut quoted = false;
    let mut has = false;
    let mut chars = s.chars();
    while let Some(c) = chars.next() {
        match c {
            // Как в cmd.exe: ^ вне кавычек экранирует следующий символ (например, ^!).
            '^' if !quoted => {
                if let Some(n) = chars.next() {
                    cur.push(n);
                    has = true;
                }
            }
            '"' => {
                quoted = !quoted;
                has = true;
            }
            c if c.is_whitespace() && !quoted => {
                if has {
                    out.push(std::mem::take(&mut cur));
                    has = false;
                }
            }
            c => {
                cur.push(c);
                has = true;
            }
        }
    }
    if has {
        out.push(cur);
    }
    out
}

/// Достать аргументы winws.exe из .bat-файла Flowseal.
pub fn parse_bat(text: &str, paths: &ZapretPaths, games: bool) -> Result<Vec<String>, String> {
    let marker = "winws.exe\"";
    let idx = text.find(marker).ok_or("В файле стратегии не найден запуск winws.exe")?;
    let mut parts = Vec::new();
    for line in text[idx + marker.len()..].lines() {
        let t = line.trim_end();
        if let Some(stripped) = t.strip_suffix('^') {
            parts.push(stripped.to_string());
        } else {
            parts.push(t.to_string());
            break;
        }
    }
    let joined = parts.join(" ");
    let bin = format!("{}\\", paths.bin().display());
    let lists = format!("{}\\", paths.lists().display());
    // Как в service.bat Flowseal: без игрового режима ставится заглушка-порт 12.
    let range = if games { "1024-65535" } else { "12" };
    let joined = joined
        .replace("%BIN%", &bin)
        .replace("%LISTS%", &lists)
        .replace("%GameFilterTCP%", range)
        .replace("%GameFilterUDP%", range)
        .replace("%GameFilter%", range);
    if joined.contains('%') {
        return Err("В стратегии осталась неизвестная переменная".into());
    }
    Ok(tokenize(&joined))
}

/// Создать пользовательские списки так же, как это делает service.bat.
pub fn ensure_user_lists(paths: &ZapretPaths) {
    let l = paths.lists();
    let files = [
        ("ipset-exclude-user.txt", "203.0.113.113/32\r\n"),
        ("list-general-user.txt", "# Never leave this file empty\r\ndomain.example.abc\r\n"),
        ("list-exclude-user.txt", "domain.example.abc\r\n"),
    ];
    for (name, body) in files {
        let p = l.join(name);
        if !p.exists() {
            let _ = fs::write(p, body);
        }
    }
}

pub fn kill_stray() {
    run_quiet("taskkill", &["/F", "/IM", "winws.exe"]);
}

#[derive(Default)]
pub struct Engine {
    child: Option<Child>,
}

impl Engine {
    pub fn start(&mut self, paths: &ZapretPaths, strategy: &str, games: bool, log: &Log) -> Result<(), String> {
        self.stop();
        kill_stray();
        ensure_user_lists(paths);
        let bat = paths.strategies().join(strategy);
        let text = fs::read_to_string(&bat).map_err(|e| format!("Не удалось прочитать стратегию: {e}"))?;
        let args = parse_bat(&text, paths, games)?;
        let exe = paths.bin().join("winws.exe");
        if !exe.exists() {
            return Err("Не найден winws.exe. Переустановите Prism.".into());
        }
        log.push(format!("[prism] запуск стратегии {strategy}, игровой режим: {games}"));
        let mut child = hidden(&exe)
            .args(&args)
            .current_dir(paths.bin())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .stdin(Stdio::null())
            .spawn()
            .map_err(|e| format!("Не удалось запустить winws.exe: {e}"))?;
        if let Some(out) = child.stdout.take() {
            log.pipe("winws", out);
        }
        if let Some(err) = child.stderr.take() {
            log.pipe("winws", err);
        }
        self.child = Some(child);
        Ok(())
    }

    /// Работает ли процесс движка прямо сейчас.
    pub fn alive(&mut self) -> bool {
        match self.child.as_mut() {
            Some(c) => matches!(c.try_wait(), Ok(None)),
            None => false,
        }
    }

    pub fn stop(&mut self) {
        if let Some(mut c) = self.child.take() {
            let _ = c.kill();
            let _ = c.wait();
        }
    }
}

/// Скопировать файлы zapret из папки программы в рабочую папку пользователя,
/// если версия изменилась. Пользовательские списки сохраняются.
pub fn install(resource: &Path, target: &Path, log: &Log) -> Result<(), String> {
    let src_ver = fs::read_to_string(resource.join("version.txt")).unwrap_or_default();
    let dst_ver = fs::read_to_string(target.join("version.txt")).unwrap_or_default();
    if !src_ver.is_empty() && src_ver == dst_ver && target.join("bin").join("winws.exe").exists() {
        return Ok(());
    }
    kill_stray();
    log.push(format!("[prism] установка файлов zapret {}", src_ver.trim()));
    copy_dir(resource, target).map_err(|e| format!("Не удалось скопировать файлы zapret: {e}"))
}

fn copy_dir(src: &Path, dst: &Path) -> std::io::Result<()> {
    fs::create_dir_all(dst)?;
    for e in fs::read_dir(src)? {
        let e = e?;
        let to = dst.join(e.file_name());
        if e.file_type()?.is_dir() {
            copy_dir(&e.path(), &to)?;
        } else {
            let name = e.file_name().to_string_lossy().to_string();
            if name.ends_with("-user.txt") && to.exists() {
                continue;
            }
            fs::copy(e.path(), &to)?;
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_flowseal_bat() {
        let bat = "set \"BIN=%~dp0bin\\\"\r\nstart \"zapret: %~n0\" /min \"%BIN%winws.exe\" --wf-tcp=80,443,%GameFilterTCP% ^\r\n--filter-udp=443 --hostlist=\"%LISTS%list-general.txt\" --dpi-desync=fake --new ^\r\n--filter-tcp=80\r\n\r\necho done\r\n";
        let p = ZapretPaths { root: PathBuf::from("C:\\Data\\Prism Dir\\zapret") };
        let args = parse_bat(bat, &p, false).unwrap();
        assert_eq!(args[0], "--wf-tcp=80,443,12");
        assert_eq!(args[2].contains("Prism Dir"), true);
        assert!(args[2].ends_with("list-general.txt"));
        assert_eq!(args.last().unwrap(), "--filter-tcp=80");
        assert_eq!(tokenize("--fake-tls=^! --x=\"a b\""), vec!["--fake-tls=!", "--x=a b"]);
        let g = parse_bat(bat, &p, true).unwrap();
        assert_eq!(g[0], "--wf-tcp=80,443,1024-65535");
    }
}

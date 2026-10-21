//! Диагностика: то, что проверяет service.bat Flowseal, но с понятными советами.
//! Ищем самые частые причины, по которым обход не работает: выключенную службу BFE,
//! другие обходы, программы, которые конфликтуют с WinDivert, VPN, системный прокси,
//! удалённые антивирусом файлы и записи YouTube в hosts.
//!
//! Команды Windows выводят текст на языке системы, поэтому разбираем только то,
//! что не переводится: коды выхода, имена служб и процессов, значения реестра.

use serde::Serialize;

use crate::engine::ZapretPaths;
use crate::hosts;
use crate::util::run_output;

#[derive(Serialize, Clone, Debug)]
pub struct Item {
    pub id: &'static str,
    /// ok, warn, bad или skip (проверить не удалось).
    pub level: &'static str,
    pub title: String,
    pub detail: String,
    /// Проблему может исправить сам Prism (команда fix_problem).
    pub fix: bool,
}

fn item(id: &'static str, level: &'static str, title: impl Into<String>, detail: impl Into<String>) -> Item {
    Item { id, level, title: title.into(), detail: detail.into(), fix: false }
}

fn fixable(mut i: Item) -> Item {
    i.fix = true;
    i
}

/// Службы других обходов: вместе с Prism они перехватывают тот же трафик через WinDivert.
const BYPASSES: &[&str] = &["zapret", "GoodbyeDPI", "discordfix_zapret", "winws1", "winws2"];

/// Программы, которые, по опыту Flowseal, ломают обход: (название, слова в имени службы).
const SOFTWARE: &[(&str, &[&str])] = &[
    ("Killer", &["killer"]),
    ("Intel Connectivity Network Service", &["intel", "connectivity", "network"]),
    ("Check Point", &["tracsrvwrapper"]),
    ("Check Point", &["epwd"]),
    ("SmartByte", &["smartbyte"]),
];

/// Без этих файлов winws.exe не запустится. Чаще всего их удаляет антивирус.
const ENGINE_FILES: &[&str] = &["winws.exe", "WinDivert.dll", "WinDivert64.sys", "cygwin1.dll"];

/// Служба из списка `sc query`: имя и отображаемое имя.
#[derive(Debug, PartialEq)]
struct Service {
    name: String,
    display: String,
}

impl Service {
    fn matches(&self, words: &[&str]) -> bool {
        let text = format!("{} {}", self.name, self.display).to_lowercase();
        words.iter().all(|w| text.contains(w))
    }

    /// Как показать службу: отображаемое имя, если оно прочиталось без искажений.
    fn label(&self) -> &str {
        if !self.display.is_empty() && self.display.is_ascii() {
            &self.display
        } else {
            &self.name
        }
    }
}

/// Разобрать вывод `sc query`. Подписи («SERVICE_NAME», «Имя_службы») зависят от языка
/// системы, поэтому опираемся на строение: у каждой службы две строки без отступа —
/// имя и отображаемое имя, остальные строки с отступом.
fn parse_services(out: &str) -> Vec<Service> {
    let values: Vec<&str> = out
        .lines()
        .filter(|l| !l.is_empty() && !l.starts_with(char::is_whitespace))
        .filter_map(|l| l.split_once(':').map(|(_, v)| v.trim()))
        .collect();
    values
        .chunks(2)
        .map(|c| Service { name: c[0].to_string(), display: c.get(1).unwrap_or(&"").to_string() })
        .collect()
}

/// Работающие службы (без аргументов `sc query` показывает только их).
fn active_services() -> Option<Vec<Service>> {
    match run_output("sc", &["query"]) {
        Some((0, out)) => Some(parse_services(&out)),
        _ => None,
    }
}

fn service_exists(name: &str) -> bool {
    // 0 — служба есть (в любом состоянии), 1060 — такой службы нет.
    matches!(run_output("sc", &["query", name]), Some((0, _)))
}

fn found_bypasses() -> Vec<&'static str> {
    BYPASSES.iter().copied().filter(|s| service_exists(s)).collect()
}

/// Есть ли процесс в выводе `tasklist /fo csv /nh`.
fn has_process(tasklist: &str, exe: &str) -> bool {
    tasklist.lines().filter_map(|l| l.split(',').next()).any(|n| n.trim_matches('"').eq_ignore_ascii_case(exe))
}

/// Значение REG_DWORD из вывода `reg query`.
fn reg_dword(out: &str, name: &str) -> Option<u32> {
    let line = out.lines().find(|l| l.split_whitespace().next().is_some_and(|n| n.eq_ignore_ascii_case(name)))?;
    let mut it = line.split_whitespace().skip(1);
    if it.next()? != "REG_DWORD" {
        return None;
    }
    u32::from_str_radix(it.next()?.trim_start_matches("0x"), 16).ok()
}

/// Значение REG_SZ из вывода `reg query`.
fn reg_sz(out: &str, name: &str) -> Option<String> {
    let line = out.lines().find(|l| l.split_whitespace().next().is_some_and(|n| n.eq_ignore_ascii_case(name)))?;
    let (_, value) = line.split_once("REG_SZ")?;
    Some(value.trim().to_string())
}

fn bfe_running() -> Option<bool> {
    // Состояние RUNNING sc печатает по-английски на любом языке системы.
    run_output("sc", &["query", "BFE"]).map(|(code, out)| code == 0 && out.contains("RUNNING"))
}

fn check_bfe() -> Item {
    match bfe_running() {
        Some(true) => item("bfe", "ok", "Служба базовой фильтрации работает", ""),
        Some(false) => fixable(item(
            "bfe",
            "bad",
            "Служба базовой фильтрации (BFE) выключена",
            "Без неё WinDivert и обход не работают. Её часто отключают программы для «ускорения» Windows.",
        )),
        None => item("bfe", "skip", "Службу базовой фильтрации проверить не удалось", ""),
    }
}

fn check_bypasses() -> Item {
    let found = found_bypasses();
    if found.is_empty() {
        return item("bypass", "ok", "Других обходов не найдено", "");
    }
    fixable(item(
        "bypass",
        "bad",
        format!("Установлены другие обходы: {}", found.join(", ")),
        "Они перехватывают тот же трафик, что и Prism, и обход перестаёт работать. Prism может удалить их службы.",
    ))
}

fn check_files(paths: &ZapretPaths) -> Item {
    let bin = paths.bin();
    let missing: Vec<&str> = ENGINE_FILES.iter().copied().filter(|f| !bin.join(f).exists()).collect();
    if missing.is_empty() {
        return item("files", "ok", "Файлы обхода на месте", "");
    }
    fixable(item(
        "files",
        "bad",
        format!("Не хватает файлов обхода: {}", missing.join(", ")),
        format!(
            "Скорее всего, их удалил антивирус. Добавьте папку {} в исключения антивируса, затем нажмите «Исправить»: Prism скачает файлы заново.",
            paths.root.display()
        ),
    ))
}

fn check_software(services: Option<&[Service]>) -> Item {
    let Some(list) = services else {
        return item("software", "skip", "Конфликтующие программы проверить не удалось", "");
    };
    let mut found: Vec<&str> = Vec::new();
    for (name, words) in SOFTWARE {
        if list.iter().any(|s| s.matches(words)) && !found.contains(name) {
            found.push(name);
        }
    }
    if found.is_empty() {
        return item("software", "ok", "Конфликтующих программ не найдено", "");
    }
    item(
        "software",
        "bad",
        format!("Обходу мешает {}", found.join(", ")),
        "Эти программы конфликтуют с WinDivert. Удалите их или отключите их службы.",
    )
}

fn check_vpn(services: Option<&[Service]>) -> Item {
    let Some(list) = services else {
        return item("vpn", "skip", "VPN проверить не удалось", "");
    };
    let found: Vec<&str> = list.iter().filter(|s| s.matches(&["vpn"]) || s.matches(&["happservice"])).map(Service::label).collect();
    if found.is_empty() {
        return item("vpn", "ok", "VPN не запущены", "");
    }
    item(
        "vpn",
        "warn",
        format!("Работает VPN: {}", found.join(", ")),
        "Некоторые VPN мешают обходу. Если что-то не открывается, выключите VPN.",
    )
}

fn check_adguard() -> Item {
    match run_output("tasklist", &["/fo", "csv", "/nh"]) {
        Some((0, out)) if has_process(&out, "AdguardSvc.exe") => item(
            "adguard",
            "warn",
            "Запущен Adguard",
            "Он может мешать Discord. Если Discord не работает, выключите Adguard.",
        ),
        Some((0, _)) => item("adguard", "ok", "Adguard не мешает", ""),
        _ => item("adguard", "skip", "Adguard проверить не удалось", ""),
    }
}

fn check_proxy() -> Item {
    const KEY: &str = r"HKCU\Software\Microsoft\Windows\CurrentVersion\Internet Settings";
    // Код 1 — значения нет, то есть прокси не настраивали.
    let enabled = match run_output("reg", &["query", KEY, "/v", "ProxyEnable"]) {
        Some((0, out)) => reg_dword(&out, "ProxyEnable") == Some(1),
        Some((1, _)) => false,
        _ => return item("proxy", "skip", "Системный прокси проверить не удалось", ""),
    };
    if !enabled {
        return item("proxy", "ok", "Системный прокси выключен", "");
    }
    let server = match run_output("reg", &["query", KEY, "/v", "ProxyServer"]) {
        Some((0, out)) => reg_sz(&out, "ProxyServer").unwrap_or_default(),
        _ => String::new(),
    };
    item(
        "proxy",
        "warn",
        format!("Включён системный прокси {server}").trim_end().to_string(),
        "Если вы им не пользуетесь, выключите его: Параметры Windows → Сеть и Интернет → Прокси-сервер.",
    )
}

fn check_hosts() -> Item {
    let lines = hosts::youtube_entries();
    if lines.is_empty() {
        return item("hosts", "ok", "В файле hosts нет записей для YouTube", "");
    }
    item(
        "hosts",
        "warn",
        "В файле hosts есть записи для YouTube",
        format!(
            "Из-за них YouTube может не открываться даже с обходом. Удалите их из {}: {}",
            hosts::hosts_path().display(),
            lines.iter().take(3).cloned().collect::<Vec<_>>().join("; ")
        ),
    )
}

/// Провести все проверки. Проблемы идут первыми.
pub fn run(paths: &ZapretPaths) -> Vec<Item> {
    let services = active_services();
    let mut items = vec![
        check_bfe(),
        check_bypasses(),
        check_files(paths),
        check_software(services.as_deref()),
        check_vpn(services.as_deref()),
        check_adguard(),
        check_proxy(),
        check_hosts(),
    ];
    items.sort_by_key(|i| match i.level {
        "bad" => 0,
        "warn" => 1,
        "ok" => 2,
        _ => 3,
    });
    items
}

/// Исправить проблему, которую нашла диагностика. Файлы обхода восстанавливает
/// main.rs (их нужно скачать). Возвращает строку для журнала.
pub fn fix(id: &str) -> Result<String, String> {
    match id {
        "bfe" => {
            run_output("sc", &["config", "BFE", "start=", "auto"]);
            run_output("sc", &["start", "BFE"]);
            if bfe_running() == Some(true) {
                Ok("служба базовой фильтрации включена".into())
            } else {
                Err("Не удалось включить службу BFE. Откройте «Службы» Windows и запустите «Служба базовой фильтрации».".into())
            }
        }
        "bypass" => {
            let found = found_bypasses();
            let mut failed = Vec::new();
            for s in &found {
                run_output("sc", &["stop", s]);
                // 1072 — служба уже помечена на удаление и исчезнет после перезагрузки.
                if !matches!(run_output("sc", &["delete", s]), Some((0 | 1072, _))) {
                    failed.push(*s);
                }
            }
            // Их драйвер WinDivert мог остаться загруженным. Prism загрузит его заново.
            for d in ["WinDivert", "WinDivert14"] {
                run_output("sc", &["stop", d]);
                run_output("sc", &["delete", d]);
            }
            if failed.is_empty() {
                Ok(format!("удалены службы других обходов: {}", found.join(", ")))
            } else {
                Err(format!("Не удалось удалить службы: {}. Перезагрузите компьютер и попробуйте ещё раз.", failed.join(", ")))
            }
        }
        _ => Err("Это Prism исправить не может".into()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SC_EN: &str = "\r\nSERVICE_NAME: Appinfo\r\nDISPLAY_NAME: Application Information\r\n        TYPE               : 30  WIN32\r\n        STATE              : 4  RUNNING\r\n                                (STOPPABLE, NOT_PAUSABLE, IGNORES_SHUTDOWN)\r\n        WIN32_EXIT_CODE    : 0  (0x0)\r\n\r\nSERVICE_NAME: OpenVPNService\r\nDISPLAY_NAME: OpenVPN Interactive Service\r\n        TYPE               : 10  WIN32_OWN_PROCESS\r\n        STATE              : 4  RUNNING\r\n\r\nSERVICE_NAME: Killer Network Service\r\nDISPLAY_NAME: Killer Network Service\r\n        STATE              : 4  RUNNING\r\n";

    // Так sc выводит службы на русской Windows.
    const SC_RU: &str = "\r\nИмя_службы: Intel(R) CNS\r\nВыводимое_имя: Intel(R) Connectivity Network Service\r\n        Тип                : 10  WIN32_OWN_PROCESS\r\n        Состояние          : 4  RUNNING\r\n\r\nИмя_службы: AudioSrv\r\nВыводимое_имя: Windows Audio\r\n        Состояние          : 4  RUNNING\r\n";

    #[test]
    fn parses_services_in_any_language() {
        let en = parse_services(SC_EN);
        assert_eq!(en.len(), 3);
        assert_eq!(en[1], Service { name: "OpenVPNService".into(), display: "OpenVPN Interactive Service".into() });
        let ru = parse_services(SC_RU);
        assert_eq!(ru.len(), 2);
        assert_eq!(ru[0].name, "Intel(R) CNS");
        assert_eq!(ru[1].display, "Windows Audio");
    }

    #[test]
    fn finds_conflicts_and_vpn() {
        let mut list = parse_services(SC_EN);
        list.extend(parse_services(SC_RU));
        let sw = check_software(Some(&list));
        assert_eq!(sw.level, "bad");
        assert_eq!(sw.title, "Обходу мешает Killer, Intel Connectivity Network Service");
        let vpn = check_vpn(Some(&list));
        assert_eq!(vpn.level, "warn");
        assert_eq!(vpn.title, "Работает VPN: OpenVPN Interactive Service");
        let clean = parse_services("SERVICE_NAME: AudioSrv\r\nDISPLAY_NAME: Windows Audio\r\n");
        assert_eq!(check_software(Some(&clean)).level, "ok");
        assert_eq!(check_vpn(Some(&clean)).level, "ok");
        assert_eq!(check_vpn(None).level, "skip");
    }

    #[test]
    fn reads_registry_values() {
        let out = "\r\nHKEY_CURRENT_USER\\Software\\Microsoft\\Windows\\CurrentVersion\\Internet Settings\r\n    ProxyEnable    REG_DWORD    0x1\r\n    ProxyServer    REG_SZ    127.0.0.1:8080\r\n";
        assert_eq!(reg_dword(out, "ProxyEnable"), Some(1));
        assert_eq!(reg_sz(out, "ProxyServer").as_deref(), Some("127.0.0.1:8080"));
        assert_eq!(reg_dword("    ProxyEnable    REG_DWORD    0x0\r\n", "ProxyEnable"), Some(0));
        assert_eq!(reg_dword(out, "ProxyOverride"), None);
    }

    #[test]
    fn finds_process() {
        let out = "\"System Idle Process\",\"0\",\"Services\",\"0\",\"8 K\"\r\n\"AdguardSvc.exe\",\"4242\",\"Services\",\"0\",\"51 200 K\"\r\n";
        assert!(has_process(out, "adguardsvc.exe"));
        assert!(!has_process(out, "goodbyedpi.exe"));
    }

    #[test]
    fn reports_missing_files() {
        let dir = std::env::temp_dir().join(format!("prism-diag-test-{}", std::process::id()));
        let paths = ZapretPaths { root: dir.clone() };
        let _ = std::fs::create_dir_all(paths.bin());
        for f in ["winws.exe", "WinDivert.dll", "cygwin1.dll"] {
            let _ = std::fs::write(paths.bin().join(f), b"x");
        }
        let i = check_files(&paths);
        assert_eq!((i.level, i.fix), ("bad", true));
        assert_eq!(i.title, "Не хватает файлов обхода: WinDivert64.sys");
        let _ = std::fs::write(paths.bin().join("WinDivert64.sys"), b"x");
        assert_eq!(check_files(&paths).level, "ok");
        let _ = std::fs::remove_dir_all(&dir);
    }
}

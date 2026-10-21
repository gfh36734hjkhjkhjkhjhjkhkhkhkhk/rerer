#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod diag;
mod engine;
mod hosts;
mod probe;
mod settings;
mod tgproxy;
mod updates;
mod util;

use std::collections::VecDeque;
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::time::{Duration, Instant};

use serde::Serialize;
use tauri::menu::{Menu, MenuItem, PredefinedMenuItem};
use tauri::tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent};
use tauri::{AppHandle, Emitter, Manager, RunEvent, State, WindowEvent, Wry};

use engine::{Engine, Strategy, ZapretPaths};
use settings::Settings;
use tgproxy::Proxy;
use util::{run_quiet, Log};

const ZAPRET_VERSION: &str = "обходы: Flowseal zapret-discord-youtube";
const TG_VERSION: &str = "tg-ws-proxy 1.11.2";
const TASK_NAME: &str = "Prism";

/// Сторож перезапускает упавший winws.exe не чаще MAX_RESTARTS раз за RESTART_WINDOW.
const MAX_RESTARTS: usize = 3;
const RESTART_WINDOW: Duration = Duration::from_secs(600);
/// Если winws.exe проработал меньше, перезапуск не поможет: стратегия не подходит.
const MIN_HEALTHY_UPTIME: Duration = Duration::from_secs(15);

struct Inner {
    settings: Settings,
    engine: Engine,
    proxy: Proxy,
    /// Длинная операция, которая сама перезапускает движок: подбор стратегии или
    /// обновление обходов. Пока она идёт, сторож и переключатели движок не трогают.
    busy: Option<&'static str>,
}

struct Prism {
    inner: Mutex<Inner>,
    log: Log,
    zapret: ZapretPaths,
    tg_exe: PathBuf,
    settings_file: PathBuf,
    hosts_cache: PathBuf,
    data_dir: PathBuf,
}

/// Снимает флаг busy, когда операция закончилась, в том числе с ошибкой.
struct Busy<'a>(&'a Prism);

impl Drop for Busy<'_> {
    fn drop(&mut self) {
        self.0.inner.lock().unwrap().busy = None;
    }
}

impl Prism {
    fn save(&self, i: &Inner) {
        i.settings.save(&self.settings_file);
    }

    /// Начать длинную операцию, если не идёт другая.
    fn begin(&self, what: &'static str) -> R<Busy<'_>> {
        let mut i = self.inner.lock().unwrap();
        if let Some(cur) = i.busy {
            return Err(format!("Подождите, сейчас идёт {cur}"));
        }
        i.busy = Some(what);
        Ok(Busy(self))
    }

    fn ensure_idle(&self) -> R<()> {
        match self.inner.lock().unwrap().busy {
            Some(cur) => Err(format!("Подождите, сейчас идёт {cur}")),
            None => Ok(()),
        }
    }
}

#[derive(Serialize)]
struct TgInfo {
    host: &'static str,
    port: u16,
    secret: String,
    link: String,
}

#[derive(Serialize)]
struct AppState {
    engine_on: bool,
    /// Когда включился обход, миллисекунды с 1970 года (0 — выключен).
    since: u64,
    strategy: String,
    strategies: Vec<Strategy>,
    games: bool,
    ai: bool,
    tg_on: bool,
    tg: TgInfo,
    autostart: bool,
    tray: bool,
    user_sites: Vec<String>,
    builtin_sites: usize,
    versions: Vec<&'static str>,
    auto_update: bool,
    app_version: &'static str,
    zapret_version: String,
    portable: bool,
}

type R<T> = Result<T, String>;

fn snapshot(p: &Prism) -> AppState {
    let mut i = p.inner.lock().unwrap();
    let engine_on = i.engine.alive();
    let since = i.engine.since_ms();
    let tg_on = i.proxy.alive();
    let s = &i.settings;
    AppState {
        engine_on,
        since,
        strategy: s.strategy.clone(),
        strategies: engine::list_strategies(&p.zapret),
        games: s.games,
        ai: s.ai,
        tg_on,
        tg: TgInfo { host: tgproxy::HOST, port: s.tg_port, secret: s.tg_secret.clone(), link: tgproxy::link(s.tg_port, &s.tg_secret) },
        autostart: s.autostart,
        tray: s.tray,
        user_sites: read_user_sites(&p.zapret),
        builtin_sites: count_lines(&p.zapret.lists().join("list-general.txt")),
        versions: vec![ZAPRET_VERSION, TG_VERSION],
        auto_update: s.auto_update,
        app_version: env!("CARGO_PKG_VERSION"),
        zapret_version: zapret_version(&p.zapret),
        portable: is_portable(),
    }
}

fn count_lines(path: &std::path::Path) -> usize {
    std::fs::read_to_string(path)
        .map(|t| t.lines().filter(|l| !l.trim().is_empty() && !l.starts_with('#')).count())
        .unwrap_or(0)
}

const PLACEHOLDER: &str = "domain.example.abc";

fn read_user_sites(z: &ZapretPaths) -> Vec<String> {
    std::fs::read_to_string(z.user_list())
        .unwrap_or_default()
        .lines()
        .map(|l| l.trim().to_string())
        .filter(|l| !l.is_empty() && !l.starts_with('#') && l != PLACEHOLDER)
        .collect()
}

fn write_user_sites(z: &ZapretPaths, sites: &[String]) -> R<()> {
    let mut body = String::from("# Never leave this file empty\r\n");
    body.push_str(PLACEHOLDER);
    body.push_str("\r\n");
    for s in sites {
        body.push_str(s);
        body.push_str("\r\n");
    }
    std::fs::write(z.user_list(), body).map_err(|e| format!("Не удалось сохранить список: {e}"))
}

/// Запустить движок и убедиться, что он не упал сразу.
async fn start_engine(p: &Prism) -> R<()> {
    {
        let mut i = p.inner.lock().unwrap();
        let (strategy, games) = (i.settings.strategy.clone(), i.settings.games);
        i.engine.start(&p.zapret, &strategy, games, &p.log)?;
    }
    tokio::time::sleep(Duration::from_millis(1500)).await;
    let mut i = p.inner.lock().unwrap();
    if !i.engine.alive() {
        if let Some(code) = i.engine.exited() {
            p.log.push(format!("[prism] winws.exe завершился с кодом {code:#x}"));
        }
        i.engine.stop();
        return Err("Обход не запустился. Откройте журнал в настройках, там написана причина.".into());
    }
    Ok(())
}

/// Остановить движок и выгрузить драйвер WinDivert.
fn stop_engine(p: &Prism) {
    p.inner.lock().unwrap().engine.stop();
    engine::kill_stray();
    engine::unload_driver();
}

/// Включить или выключить обход и запомнить выбор.
async fn switch_engine(p: &Prism, on: bool) -> R<()> {
    p.ensure_idle()?;
    if on {
        start_engine(p).await?;
    } else {
        stop_engine(p);
    }
    let mut i = p.inner.lock().unwrap();
    i.settings.engine_on = on;
    p.save(&i);
    Ok(())
}

fn start_proxy(p: &Prism) -> R<()> {
    let mut i = p.inner.lock().unwrap();
    let (port, secret) = (i.settings.tg_port, i.settings.tg_secret.clone());
    i.proxy.start(&p.tg_exe, port, &secret, &p.log)
}

fn zapret_version(z: &ZapretPaths) -> String {
    std::fs::read_to_string(z.root.join("version.txt"))
        .unwrap_or_default()
        .trim()
        .trim_start_matches("flowseal-")
        .to_string()
}

/// Установленная версия лежит рядом с uninstall.exe, портативная — нет.
fn is_portable() -> bool {
    std::env::current_exe()
        .ok()
        .and_then(|e| e.parent().map(|d| !d.join("uninstall.exe").exists()))
        .unwrap_or(true)
}

fn open_url(url: &str) {
    let _ = util::hidden("rundll32").args(["url.dll,FileProtocolHandler", url]).spawn();
}

async fn check_info(p: &Prism) -> R<updates::Info> {
    let (app_r, zap_r) = tokio::join!(updates::latest(updates::APP_REPO), updates::latest(updates::ZAPRET_REPO));
    let cur_app = env!("CARGO_PKG_VERSION").to_string();
    let cur_zap = zapret_version(&p.zapret);
    let mut info = updates::Info::default();
    info.app.current = cur_app.clone();
    info.zapret.current = cur_zap.clone();
    if let Ok(r) = &app_r {
        info.app.latest = r.tag_name.trim_start_matches('v').to_string();
        info.app.available = updates::newer(&r.tag_name, &cur_app) && updates::app_asset(r).is_some();
    }
    if let Ok(r) = &zap_r {
        info.zapret.latest = r.tag_name.trim_start_matches('v').to_string();
        info.zapret.available = updates::newer(&r.tag_name, &cur_zap) && updates::zapret_asset(r).is_some();
    }
    if let (Err(e), Err(_)) = (&app_r, &zap_r) {
        return Err(e.clone());
    }
    Ok(info)
}

/// Скачать и установить обходы Flowseal. Возвращает true, если файлы заменены.
/// force — переустановить и ту же версию, например если антивирус удалил файлы.
async fn do_update_zapret(p: &Prism, force: bool) -> R<bool> {
    let rel = updates::latest(updates::ZAPRET_REPO).await?;
    let current = zapret_version(&p.zapret);
    if !force && !updates::newer(&rel.tag_name, &current) {
        return Ok(false);
    }
    let _busy = p.begin("обновление обходов")?;
    let asset = updates::zapret_asset(&rel).ok_or("В релизе Flowseal нет архива zapret")?.clone();
    p.log.push(format!("[prism] скачиваю обходы Flowseal {}", rel.tag_name));
    let bytes = updates::download_verified(&asset).await?;
    let root = p.zapret.root.clone();
    let fresh = root.with_file_name("zapret-new");
    let old = root.with_file_name("zapret-old");
    updates::unpack_zapret(&bytes, &fresh, &rel.tag_name)?;
    if let Err(e) = check_fresh(p, &fresh, &rel.tag_name) {
        let _ = std::fs::remove_dir_all(&fresh);
        return Err(e);
    }
    updates::carry_user_lists(&root, &fresh);
    let running = {
        let mut i = p.inner.lock().unwrap();
        let r = i.engine.alive();
        i.engine.stop();
        r
    };
    engine::kill_stray();
    engine::unload_driver();
    tokio::time::sleep(Duration::from_millis(500)).await;
    if let Err(e) = swap_dirs(&root, &fresh, &old) {
        let _ = std::fs::remove_dir_all(&fresh);
        // Файлы остались прежними: возвращаем обход, который работал до обновления.
        if running {
            if let Err(e) = start_engine(p).await {
                p.log.push(format!("[prism] {e}"));
            }
        }
        return Err(e);
    }
    {
        let mut i = p.inner.lock().unwrap();
        if !p.zapret.strategies().join(&i.settings.strategy).exists() {
            i.settings.strategy = "general.bat".into();
            p.save(&i);
        }
    }
    p.log.push(format!("[prism] обходы обновлены до {}", rel.tag_name));
    if running {
        start_engine(p).await?;
    }
    Ok(true)
}

/// Новая версия Flowseal может сменить формат стратегий. Проверяем её до замены
/// файлов, чтобы автообновление не оставило всех без обхода.
fn check_fresh(p: &Prism, fresh: &Path, tag: &str) -> R<()> {
    let paths = ZapretPaths { root: fresh.to_path_buf() };
    let current = p.inner.lock().unwrap().settings.strategy.clone();
    let broken = engine::broken_strategies(&paths);
    for (id, e) in &broken {
        p.log.push(format!("[prism] Flowseal {tag}, {id}: {e}"));
    }
    let essential = broken.iter().any(|(id, _)| *id == current || id == "general.bat");
    if essential || engine::list_strategies(&paths).is_empty() {
        return Err(format!("Обходы Flowseal {tag} пока не поддерживаются этой версией Prism, обновление отложено"));
    }
    Ok(())
}

/// Заменить папку root на fresh. Если не вышло, root остаётся прежней.
fn swap_dirs(root: &Path, fresh: &Path, old: &Path) -> R<()> {
    let _ = std::fs::remove_dir_all(old);
    let had_root = root.exists();
    if had_root {
        std::fs::rename(root, old).map_err(|e| format!("Не удалось заменить файлы: {e}"))?;
    }
    if let Err(e) = std::fs::rename(fresh, root) {
        if had_root {
            let _ = std::fs::rename(old, root);
        }
        return Err(format!("Не удалось заменить файлы: {e}"));
    }
    let _ = std::fs::remove_dir_all(old);
    Ok(())
}

/// Адреса для нейросетей: свежие из сети или сохранённые в прошлый раз.
async fn ai_list(p: &Prism) -> R<String> {
    match hosts::download().await {
        Ok(list) => {
            let _ = std::fs::write(&p.hosts_cache, &list);
            Ok(list)
        }
        Err(e) => std::fs::read_to_string(&p.hosts_cache).map_err(|_| e),
    }
}

#[tauri::command]
async fn check_updates(p: State<'_, Prism>) -> R<updates::Info> {
    check_info(&p).await
}

#[tauri::command]
async fn update_zapret(p: State<'_, Prism>) -> R<AppState> {
    if !do_update_zapret(&p, false).await? {
        return Err("У вас уже последняя версия обходов".into());
    }
    Ok(snapshot(&p))
}

#[tauri::command]
async fn update_app(app: AppHandle, p: State<'_, Prism>) -> R<()> {
    let rel = updates::latest(updates::APP_REPO).await?;
    if !updates::newer(&rel.tag_name, env!("CARGO_PKG_VERSION")) {
        return Err("У вас уже последняя версия Prism".into());
    }
    if is_portable() {
        open_url(&rel.html_url);
        return Ok(());
    }
    let asset = updates::app_asset(&rel).ok_or("В релизе нет установщика")?.clone();
    p.log.push(format!("[prism] скачиваю Prism {}", rel.tag_name));
    let bytes = updates::download_verified(&asset).await?;
    let path = std::env::temp_dir().join(&asset.name);
    std::fs::write(&path, bytes).map_err(|e| format!("Не удалось сохранить установщик: {e}"))?;
    // tg-ws-proxy.exe лежит в папке программы: установщик будет его заменять.
    let tg_was_on = {
        let mut i = p.inner.lock().unwrap();
        let on = i.proxy.alive();
        i.proxy.stop();
        on
    };
    // Так обновляет сам Tauri: /P — только полоса прогресса, /UPDATE — поверх
    // установленной версии, /R — снова запустить Prism, и обход включится сам.
    if let Err(e) = std::process::Command::new(&path).args(["/P", "/UPDATE", "/R"]).spawn() {
        if tg_was_on {
            let _ = start_proxy(&p);
        }
        return Err(format!("Не удалось запустить установщик: {e}"));
    }
    p.log.push(format!("[prism] устанавливаю Prism {}, программа перезапустится", rel.tag_name));
    shutdown(&app);
    app.exit(0);
    Ok(())
}

#[tauri::command]
async fn set_auto_update(p: State<'_, Prism>, on: bool) -> R<AppState> {
    {
        let mut i = p.inner.lock().unwrap();
        i.settings.auto_update = on;
        p.save(&i);
    }
    Ok(snapshot(&p))
}

#[tauri::command]
async fn get_state(p: State<'_, Prism>) -> R<AppState> {
    Ok(snapshot(&p))
}

#[tauri::command]
async fn set_engine(p: State<'_, Prism>, on: bool) -> R<AppState> {
    switch_engine(&p, on).await?;
    Ok(snapshot(&p))
}

#[tauri::command]
async fn set_strategy(p: State<'_, Prism>, id: String) -> R<AppState> {
    p.ensure_idle()?;
    let running = {
        let mut i = p.inner.lock().unwrap();
        i.settings.strategy = id;
        p.save(&i);
        i.engine.alive()
    };
    if running {
        start_engine(&p).await?;
    }
    Ok(snapshot(&p))
}

#[tauri::command]
async fn set_games(p: State<'_, Prism>, on: bool) -> R<AppState> {
    p.ensure_idle()?;
    let running = {
        let mut i = p.inner.lock().unwrap();
        i.settings.games = on;
        p.save(&i);
        i.engine.alive()
    };
    if running {
        start_engine(&p).await?;
    }
    Ok(snapshot(&p))
}

#[tauri::command]
async fn set_ai(p: State<'_, Prism>, on: bool) -> R<AppState> {
    if on {
        hosts::enable(&ai_list(&p).await?)?;
    } else {
        hosts::disable()?;
    }
    {
        let mut i = p.inner.lock().unwrap();
        i.settings.ai = on;
        p.save(&i);
    }
    p.log.push(format!("[prism] нейросети: {}", if on { "включено" } else { "выключено" }));
    Ok(snapshot(&p))
}

#[tauri::command]
async fn set_tg(p: State<'_, Prism>, on: bool) -> R<AppState> {
    {
        let mut i = p.inner.lock().unwrap();
        if on {
            if i.settings.tg_secret.len() != 32 {
                i.settings.tg_secret = tgproxy::new_secret();
            }
            let (port, secret) = (i.settings.tg_port, i.settings.tg_secret.clone());
            i.proxy.start(&p.tg_exe, port, &secret, &p.log)?;
        } else {
            i.proxy.stop();
        }
        i.settings.tg_on = on;
        p.save(&i);
    }
    if on {
        tokio::time::sleep(Duration::from_millis(1200)).await;
        let mut i = p.inner.lock().unwrap();
        if !i.proxy.alive() {
            i.settings.tg_on = false;
            p.save(&i);
            return Err("Прокси не запустился. Возможно, порт занят другой программой. Подробности в журнале.".into());
        }
    }
    Ok(snapshot(&p))
}

#[tauri::command]
async fn open_tg_link(p: State<'_, Prism>) -> R<()> {
    let link = {
        let i = p.inner.lock().unwrap();
        tgproxy::link(i.settings.tg_port, &i.settings.tg_secret)
    };
    // rundll32 открывает ссылку tg:// в Telegram без участия cmd (символ & не ломается).
    util::hidden("rundll32").args(["url.dll,FileProtocolHandler", &link]).spawn().map_err(|e| e.to_string())?;
    Ok(())
}

/// Адрес сайта для списка zapret: без протокола, пути и www., русские домены — в punycode.
fn normalize_domain(raw: &str) -> Option<String> {
    let mut d = raw.trim().to_lowercase();
    for pre in ["https://", "http://"] {
        if let Some(rest) = d.strip_prefix(pre) {
            d = rest.to_string();
        }
    }
    let d = d.split(['/', '?', '#', ':']).next().unwrap_or("").trim_start_matches("www.");
    if d.is_empty() {
        return None;
    }
    // zapret сравнивает домены в том виде, в каком их видно в трафике: сайт.рф → xn--80aswg.xn--p1ai.
    let host = reqwest::Url::parse(&format!("http://{d}/")).ok()?.host_str()?.to_string();
    let valid = host.contains('.')
        && !host.starts_with('.')
        && !host.ends_with('.')
        && !host.contains("..")
        && host.chars().all(|c| c.is_ascii_alphanumeric() || c == '.' || c == '-')
        && host.parse::<std::net::Ipv4Addr>().is_err();
    valid.then_some(host)
}

/// Разобрать ввод: можно вставить сразу несколько адресов через пробел, запятую или с новой строки.
fn parse_sites(raw: &str) -> R<Vec<String>> {
    let mut out: Vec<String> = Vec::new();
    for part in raw.split(|c: char| c.is_whitespace() || c == ',' || c == ';').filter(|s| !s.is_empty()) {
        let d = normalize_domain(part).ok_or_else(|| format!("«{part}» не похоже на адрес сайта. Пример: site.ru"))?;
        if !out.contains(&d) {
            out.push(d);
        }
    }
    if out.is_empty() {
        return Err("Введите адрес сайта, например site.ru".into());
    }
    Ok(out)
}

#[tauri::command]
async fn add_site(p: State<'_, Prism>, domain: String) -> R<AppState> {
    let new = parse_sites(&domain)?;
    let current = read_user_sites(&p.zapret);
    let mut sites: Vec<String> = new.iter().filter(|d| !current.contains(d)).cloned().collect();
    if sites.is_empty() {
        return Err(match new.as_slice() {
            [one] => format!("{one} уже в списке"),
            _ => "Эти сайты уже в списке".into(),
        });
    }
    sites.extend(current);
    write_user_sites(&p.zapret, &sites)?;
    Ok(snapshot(&p))
}

#[tauri::command]
async fn remove_site(p: State<'_, Prism>, domain: String) -> R<AppState> {
    let sites: Vec<String> = read_user_sites(&p.zapret).into_iter().filter(|s| *s != domain).collect();
    write_user_sites(&p.zapret, &sites)?;
    Ok(snapshot(&p))
}

#[derive(Serialize)]
struct Status {
    discord: probe::Check,
    youtube: probe::Check,
    telegram: probe::Check,
    chatgpt: bool,
}

#[tauri::command]
async fn check_services(p: State<'_, Prism>) -> R<Status> {
    let (port, ai) = {
        let i = p.inner.lock().unwrap();
        (i.settings.tg_port, i.settings.ai)
    };
    let s = probe::services(6).await;
    let telegram = probe::tcp(port).await;
    let chatgpt = ai && hosts::is_enabled();
    Ok(Status { discord: s.discord, youtube: s.youtube, telegram, chatgpt })
}

#[derive(Clone, Serialize)]
struct Progress {
    id: String,
    state: &'static str,
    ms: u64,
}

#[tauri::command]
async fn auto_pick(app: AppHandle, p: State<'_, Prism>) -> R<AppState> {
    let busy = p.begin("подбор стратегии")?;
    let list = engine::list_strategies(&p.zapret);
    let (previous, was_on) = {
        let mut i = p.inner.lock().unwrap();
        (i.settings.strategy.clone(), i.engine.alive())
    };
    let mut best: Option<(usize, u64, String)> = None;
    for s in &list {
        let _ = app.emit("pick-progress", Progress { id: s.id.clone(), state: "wait", ms: 0 });
        p.inner.lock().unwrap().settings.strategy = s.id.clone();
        let started = start_engine(&p).await.is_ok();
        // Оценка важнее «работает/не работает»: для YouTube считаем, сколько адресов
        // ответило из проверенных. Стратегия, где открывается сайт, но не видео, хуже.
        let (score, ms, failed_yt) = if started {
            let mut r = probe::services(4).await;
            // Сразу после перезапуска движка часть соединений ещё не поднялась.
            // Нулевой результат повторяем один раз, чтобы не отбросить рабочую стратегию.
            if !r.discord.ok && !r.youtube.ok {
                tokio::time::sleep(Duration::from_millis(900)).await;
                r = probe::services(4).await;
            }
            let svc = [&r.discord, &r.youtube];
            let ok = svc.iter().filter(|x| x.ok).count();
            let adr = svc.iter().map(|x| x.good as usize).sum::<usize>();
            let ms = if ok > 0 { (r.discord.ms + r.youtube.ms) / ok as u64 } else { 0 };
            (ok * 100 + adr, ms, r.youtube.failed.join(", "))
        } else {
            (0, 0, String::new())
        };
        let state = if score >= 200 { "ok" } else if score > 0 { "part" } else { "bad" };
        let _ = app.emit("pick-progress", Progress { id: s.id.clone(), state, ms });
        p.log.push(format!(
            "[prism] {}: оценка {score}, {ms} мс{}",
            s.name,
            if failed_yt.is_empty() { String::new() } else { format!(", YouTube не открылся: {failed_yt}") }
        ));
        let better = match &best {
            None => score > 0,
            Some((bs, bms, _)) => score > *bs || (score == *bs && ms < *bms),
        };
        if better {
            best = Some((score, ms, s.id.clone()));
        }
    }
    let found = best.is_some();
    {
        let mut i = p.inner.lock().unwrap();
        i.settings.strategy = best.map(|b| b.2).unwrap_or(previous);
        if found {
            i.settings.engine_on = true;
        }
        p.save(&i);
    }
    // Ничего не подошло — возвращаем всё как было до подбора.
    if found || was_on {
        start_engine(&p).await?;
    } else {
        stop_engine(&p);
    }
    drop(busy);
    if !found {
        p.log.push("[prism] подбор: ни одна стратегия не открыла Discord и YouTube");
        return Err("Ни одна стратегия не открыла Discord и YouTube. Запустите диагностику в настройках: возможно, обходу мешает другая программа.".into());
    }
    Ok(snapshot(&p))
}

async fn diagnose(p: &Prism) -> R<Vec<diag::Item>> {
    let paths = p.zapret.clone();
    let items = tauri::async_runtime::spawn_blocking(move || diag::run(&paths)).await.map_err(|e| e.to_string())?;
    let issues: Vec<&str> = items.iter().filter(|i| i.level == "bad" || i.level == "warn").map(|i| i.title.as_str()).collect();
    p.log.push(if issues.is_empty() {
        "[prism] диагностика: проблем не найдено".to_string()
    } else {
        format!("[prism] диагностика: {}", issues.join("; "))
    });
    Ok(items)
}

#[tauri::command]
async fn run_diagnostics(p: State<'_, Prism>) -> R<Vec<diag::Item>> {
    diagnose(&p).await
}

#[tauri::command]
async fn fix_problem(p: State<'_, Prism>, id: String) -> R<Vec<diag::Item>> {
    if id == "files" {
        do_update_zapret(&p, true).await?;
    } else {
        let busy = p.begin("исправление")?;
        // Другие обходы делят с Prism драйвер WinDivert: на время удаления их служб движок останавливаем.
        let running = id == "bypass" && p.inner.lock().unwrap().engine.alive();
        if running {
            stop_engine(&p);
        }
        let what = id.clone();
        let result = tauri::async_runtime::spawn_blocking(move || diag::fix(&what)).await.map_err(|e| e.to_string())?;
        if running {
            if let Err(e) = start_engine(&p).await {
                p.log.push(format!("[prism] {e}"));
            }
        }
        drop(busy);
        p.log.push(format!("[prism] исправление: {}", result.as_ref().unwrap_or_else(|e| e)));
        result?;
    }
    diagnose(&p).await
}

#[tauri::command]
async fn open_data_dir(p: State<'_, Prism>) -> R<()> {
    std::process::Command::new("explorer").arg(&p.data_dir).spawn().map(|_| ()).map_err(|e| e.to_string())
}

fn xml_escape(s: &str) -> String {
    s.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;").replace('"', "&quot;").replace('\'', "&apos;")
}

/// Задание планировщика для автозапуска: при входе в Windows, с правами администратора и без окна UAC.
/// В отличие от простого `schtasks /SC ONLOGON` оно запускается и от батареи,
/// не останавливается через 3 дня и работает с обычным приоритетом.
fn task_xml(exe: &std::path::Path) -> String {
    let user = match (std::env::var("USERDOMAIN"), std::env::var("USERNAME")) {
        (Ok(d), Ok(u)) if !d.is_empty() && !u.is_empty() => format!("{d}\\{u}"),
        (_, Ok(u)) => u,
        _ => String::new(),
    };
    let user = if user.is_empty() { String::new() } else { format!("<UserId>{}</UserId>", xml_escape(&user)) };
    let dir = exe.parent().map(|d| d.display().to_string()).unwrap_or_default();
    format!(
        r#"<?xml version="1.0" encoding="UTF-16"?>
<Task version="1.2" xmlns="http://schemas.microsoft.com/windows/2004/02/mit/task">
  <RegistrationInfo>
    <Description>Запускает Prism при входе в Windows и сразу включает обход</Description>
  </RegistrationInfo>
  <Triggers>
    <LogonTrigger>
      <Enabled>true</Enabled>
      {user}
    </LogonTrigger>
  </Triggers>
  <Principals>
    <Principal id="Author">
      {user}
      <LogonType>InteractiveToken</LogonType>
      <RunLevel>HighestAvailable</RunLevel>
    </Principal>
  </Principals>
  <Settings>
    <MultipleInstancesPolicy>IgnoreNew</MultipleInstancesPolicy>
    <DisallowStartIfOnBatteries>false</DisallowStartIfOnBatteries>
    <StopIfGoingOnBatteries>false</StopIfGoingOnBatteries>
    <AllowHardTerminate>true</AllowHardTerminate>
    <StartWhenAvailable>false</StartWhenAvailable>
    <RunOnlyIfNetworkAvailable>false</RunOnlyIfNetworkAvailable>
    <AllowStartOnDemand>true</AllowStartOnDemand>
    <Enabled>true</Enabled>
    <Hidden>false</Hidden>
    <RunOnlyIfIdle>false</RunOnlyIfIdle>
    <WakeToRun>false</WakeToRun>
    <ExecutionTimeLimit>PT0S</ExecutionTimeLimit>
    <Priority>4</Priority>
  </Settings>
  <Actions Context="Author">
    <Exec>
      <Command>{cmd}</Command>
      <Arguments>--autostart</Arguments>
      <WorkingDirectory>{dir}</WorkingDirectory>
    </Exec>
  </Actions>
</Task>
"#,
        user = user,
        cmd = xml_escape(&exe.display().to_string()),
        dir = xml_escape(&dir),
    )
}

fn set_autostart_task(on: bool, log: &Log) -> R<()> {
    if !on {
        run_quiet("schtasks", &["/Delete", "/F", "/TN", TASK_NAME]);
        log.push("[prism] автозапуск выключен");
        return Ok(());
    }
    let exe = std::env::current_exe().map_err(|e| e.to_string())?;
    // schtasks читает XML в UTF-16 с меткой порядка байтов, как его сохраняет сам планировщик.
    let mut bytes = vec![0xFF, 0xFE];
    for unit in task_xml(&exe).encode_utf16() {
        bytes.extend_from_slice(&unit.to_le_bytes());
    }
    let file = std::env::temp_dir().join("prism-autostart.xml");
    let file_s = file.display().to_string();
    let ok = std::fs::write(&file, &bytes).is_ok()
        && run_quiet("schtasks", &["/Create", "/F", "/TN", TASK_NAME, "/XML", &file_s]);
    let _ = std::fs::remove_file(&file);
    if ok {
        log.push("[prism] автозапуск включён: Prism запустится при входе в Windows и включит обход");
        return Ok(());
    }
    // Запасной вариант: простое задание при входе в систему.
    let tr = format!("\"{}\" --autostart", exe.display());
    if run_quiet("schtasks", &["/Create", "/F", "/TN", TASK_NAME, "/TR", &tr, "/SC", "ONLOGON", "/RL", "HIGHEST"]) {
        log.push("[prism] автозапуск включён (простое задание планировщика)");
        return Ok(());
    }
    Err("Не удалось включить автозапуск. Подробности в журнале.".into())
}

#[tauri::command]
async fn set_autostart(p: State<'_, Prism>, on: bool) -> R<AppState> {
    set_autostart_task(on, &p.log)?;
    {
        let mut i = p.inner.lock().unwrap();
        i.settings.autostart = on;
        p.save(&i);
    }
    Ok(snapshot(&p))
}

#[tauri::command]
async fn set_tray(p: State<'_, Prism>, on: bool) -> R<AppState> {
    {
        let mut i = p.inner.lock().unwrap();
        i.settings.tray = on;
        p.save(&i);
    }
    Ok(snapshot(&p))
}

#[tauri::command]
async fn get_log(p: State<'_, Prism>) -> R<String> {
    Ok(p.log.text())
}

#[tauri::command]
async fn close_window(app: AppHandle, p: State<'_, Prism>) -> R<()> {
    let tray = p.inner.lock().unwrap().settings.tray;
    if tray {
        if let Some(w) = app.get_webview_window("main") {
            let _ = w.hide();
        }
    } else {
        shutdown(&app);
        app.exit(0);
    }
    Ok(())
}

fn show_main(app: &AppHandle) {
    if let Some(w) = app.get_webview_window("main") {
        let _ = w.show();
        let _ = w.unminimize();
        let _ = w.set_focus();
    }
}

fn shutdown(app: &AppHandle) {
    if let Some(p) = app.try_state::<Prism>() {
        let mut i = p.inner.lock().unwrap();
        i.engine.stop();
        i.proxy.stop();
    }
    engine::kill_stray();
    engine::unload_driver();
}

/// Переключить обход из меню трея, не открывая окно.
fn toggle_from_tray(app: &AppHandle) {
    let app = app.clone();
    tauri::async_runtime::spawn(async move {
        let p = app.state::<Prism>();
        let on = !p.inner.lock().unwrap().engine.alive();
        if let Err(e) = switch_engine(&p, on).await {
            p.log.push(format!("[prism] {e}"));
            let _ = app.emit("toast", e);
        }
        let _ = app.emit("state-changed", ());
    });
}

/// Код выхода и время работы winws.exe, если он завершился сам, а не по команде Prism.
fn crashed(p: &Prism) -> Option<(i32, Duration)> {
    let mut i = p.inner.lock().unwrap();
    // Подбор и обновление сами перезапускают движок, а первые секунды после запуска проверяет start_engine.
    if i.busy.is_some() || !i.settings.engine_on || i.engine.uptime() < Duration::from_secs(3) {
        return None;
    }
    let code = i.engine.exited()?;
    let up = i.engine.uptime();
    i.engine.stop();
    Some((code, up))
}

/// Сторож: подсказка и меню трея, обновление окна и перезапуск упавшего winws.exe
/// (например, после сна компьютера или если его закрыл антивирус).
fn spawn_supervisor(app: AppHandle, toggle: MenuItem<Wry>) {
    tauri::async_runtime::spawn(async move {
        let mut last: Option<bool> = None;
        let mut restarts: VecDeque<Instant> = VecDeque::new();
        loop {
            tokio::time::sleep(Duration::from_secs(2)).await;
            let p = app.state::<Prism>();
            if let Some((code, up)) = crashed(&p) {
                restarts.retain(|t| t.elapsed() < RESTART_WINDOW);
                if up >= MIN_HEALTHY_UPTIME && restarts.len() < MAX_RESTARTS {
                    restarts.push_back(Instant::now());
                    p.log.push(format!("[prism] обход неожиданно остановился (код {code:#x}), перезапускаю"));
                    if let Err(e) = start_engine(&p).await {
                        p.log.push(format!("[prism] {e}"));
                        let _ = app.emit("toast", "Обход остановился и не перезапустился. Подробности в журнале");
                    }
                } else {
                    p.log.push(format!("[prism] обход остановился (код {code:#x}) и больше не перезапускается"));
                    let _ = app.emit("toast", "Обход остановился. Попробуйте другую стратегию или диагностику в настройках");
                }
            }
            let (on, busy) = {
                let mut i = p.inner.lock().unwrap();
                (i.engine.alive(), i.busy.is_some())
            };
            // Во время подбора движок то включается, то выключается: окно об этом не дёргаем.
            if busy || last == Some(on) {
                continue;
            }
            if let Some(t) = app.tray_by_id("main") {
                let _ = t.set_tooltip(Some(if on { "Prism: обход включён" } else { "Prism: обход выключен" }));
            }
            let _ = toggle.set_text(if on { "Выключить обход" } else { "Включить обход" });
            if last.is_some() {
                let _ = app.emit("state-changed", ());
            }
            last = Some(on);
        }
    });
}

fn setup(app: &mut tauri::App) -> Result<(), Box<dyn std::error::Error>> {
    let log = Log::default();
    let res = app.path().resource_dir()?.join("resources");
    let data = app.path().app_local_data_dir()?;
    std::fs::create_dir_all(&data)?;
    log.attach_file(&data.join("prism.log"));
    log.push(format!("[prism] Prism {} запущен", env!("CARGO_PKG_VERSION")));
    let zapret = ZapretPaths { root: data.join("zapret") };
    if let Err(e) = engine::install(&res.join("zapret"), &zapret.root, &log) {
        log.push(format!("[prism] {e}"));
    }
    let settings_file = data.join("settings.json");
    let mut settings = Settings::load(&settings_file);
    if settings.tg_secret.len() != 32 {
        settings.tg_secret = tgproxy::new_secret();
    }
    // Запуск при входе в Windows: всё должно заработать сразу, поэтому обход включается,
    // даже если в прошлый раз его выключили вручную.
    let autostart = std::env::args().any(|a| a == "--autostart");
    if autostart {
        settings.engine_on = true;
        log.push("[prism] запуск вместе с Windows");
    }
    settings.save(&settings_file);
    let restore = (settings.engine_on, settings.tg_on);
    let keep_task = settings.autostart;

    app.manage(Prism {
        inner: Mutex::new(Inner { settings, engine: Engine::default(), proxy: Proxy::default(), busy: None }),
        log,
        zapret,
        tg_exe: res.join("tgproxy").join("tg-ws-proxy.exe"),
        settings_file,
        hosts_cache: data.join("ai-hosts.txt"),
        data_dir: data,
    });

    // Трей: обход включается и выключается прямо из меню, без окна.
    let toggle = MenuItem::with_id(app, "toggle", "Включить обход", true, None::<&str>)?;
    let show = MenuItem::with_id(app, "show", "Открыть Prism", true, None::<&str>)?;
    let quit = MenuItem::with_id(app, "quit", "Выйти", true, None::<&str>)?;
    let separator = PredefinedMenuItem::separator(app)?;
    let menu = Menu::with_items(app, &[&toggle, &separator, &show, &quit])?;
    TrayIconBuilder::with_id("main")
        .icon(app.default_window_icon().cloned().ok_or("no icon")?)
        .tooltip("Prism")
        .menu(&menu)
        .show_menu_on_left_click(false)
        .on_menu_event(|app, e| match e.id.as_ref() {
            "toggle" => toggle_from_tray(app),
            "show" => show_main(app),
            "quit" => {
                shutdown(app);
                app.exit(0);
            }
            _ => {}
        })
        .on_tray_icon_event(|tray, e| {
            if let TrayIconEvent::Click { button: MouseButton::Left, button_state: MouseButtonState::Up, .. } = e {
                show_main(tray.app_handle());
            }
        })
        .build(app)?;

    if !autostart {
        show_main(app.handle());
    }

    spawn_supervisor(app.handle().clone(), toggle);

    // Вернуть то, что было включено в прошлый раз.
    let handle = app.handle().clone();
    tauri::async_runtime::spawn(async move {
        let p = handle.state::<Prism>();
        if restore.0 {
            if let Err(e) = start_engine(&p).await {
                p.log.push(format!("[prism] {e}"));
            }
        }
        if restore.1 {
            if let Err(e) = start_proxy(&p) {
                p.log.push(format!("[prism] {e}"));
            }
        }
        let _ = handle.emit("state-changed", ());

        // Обновить задание автозапуска: путь к программе мог измениться,
        // а задания из старых версий не запускались от батареи.
        if keep_task {
            if let Err(e) = set_autostart_task(true, &p.log) {
                p.log.push(format!("[prism] {e}"));
            }
        }

        // Фоновые обновления через несколько секунд после запуска.
        tokio::time::sleep(Duration::from_secs(8)).await;
        let (auto, ai) = {
            let i = p.inner.lock().unwrap();
            (i.settings.auto_update, i.settings.ai)
        };
        if auto {
            match do_update_zapret(&p, false).await {
                Ok(true) => {
                    let _ = handle.emit("zapret-updated", zapret_version(&p.zapret));
                    let _ = handle.emit("state-changed", ());
                }
                Ok(false) => {}
                Err(e) => p.log.push(format!("[prism] автообновление обходов: {e}")),
            }
        }
        // Адреса нейросетей обновляем, а если блок пропал из hosts (антивирус,
        // переустановка Windows), возвращаем его, раз режим включён.
        if ai && (auto || !hosts::is_enabled()) {
            match ai_list(&p).await.and_then(|list| hosts::enable(&list)) {
                Ok(()) => p.log.push("[prism] адреса для нейросетей обновлены"),
                Err(e) => p.log.push(format!("[prism] нейросети: {e}")),
            }
        }
        if let Ok(info) = check_info(&p).await {
            let _ = handle.emit("updates", info);
        }
    });
    Ok(())
}

fn main() {
    let app = tauri::Builder::default()
        .plugin(tauri_plugin_single_instance::init(|app, _args, _cwd| show_main(app)))
        .setup(setup)
        .on_window_event(|w, e| {
            if let WindowEvent::CloseRequested { api, .. } = e {
                let tray = w.app_handle().try_state::<Prism>().map(|p| p.inner.lock().unwrap().settings.tray).unwrap_or(false);
                if tray {
                    api.prevent_close();
                    let _ = w.hide();
                }
            }
        })
        .invoke_handler(tauri::generate_handler![
            get_state, set_engine, set_strategy, set_games, set_ai, set_tg, open_tg_link,
            add_site, remove_site, check_services, auto_pick, set_autostart, set_tray, get_log, close_window,
            check_updates, update_zapret, update_app, set_auto_update,
            run_diagnostics, fix_problem, open_data_dir
        ])
        .build(tauri::generate_context!())
        .expect("ошибка запуска Prism");

    app.run(|app, e| {
        if let RunEvent::Exit = e {
            shutdown(app);
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalizes_domains() {
        assert_eq!(normalize_domain("https://www.RuTracker.org/forum/index.php").as_deref(), Some("rutracker.org"));
        assert_eq!(normalize_domain("nnmclub.to:443").as_deref(), Some("nnmclub.to"));
        assert_eq!(normalize_domain("http://сайт.рф/путь").as_deref(), Some("xn--80aswg.xn--p1ai"));
        assert_eq!(normalize_domain("1.2.3.4"), None);
        assert_eq!(normalize_domain("localhost"), None);
        assert_eq!(normalize_domain("a..b.com"), None);
        assert_eq!(normalize_domain("bad_name!.com"), None);
        assert_eq!(normalize_domain("https://"), None);
    }

    #[test]
    fn parses_several_sites() {
        assert_eq!(
            parse_sites("rutracker.org, nnmclub.to\nhttps://rutracker.org/ x.com").unwrap(),
            vec!["rutracker.org", "nnmclub.to", "x.com"]
        );
        assert_eq!(parse_sites("  ").unwrap_err(), "Введите адрес сайта, например site.ru");
        assert!(parse_sites("ok.ru нет").unwrap_err().contains("«нет»"));
    }

    #[test]
    fn swaps_dirs_or_keeps_old() {
        let base = std::env::temp_dir().join(format!("prism-swap-test-{}", std::process::id()));
        let (root, fresh, old) = (base.join("zapret"), base.join("zapret-new"), base.join("zapret-old"));
        let _ = std::fs::create_dir_all(&root);
        let _ = std::fs::create_dir_all(&fresh);
        std::fs::write(root.join("version.txt"), "old").unwrap();
        std::fs::write(fresh.join("version.txt"), "new").unwrap();
        swap_dirs(&root, &fresh, &old).unwrap();
        assert_eq!(std::fs::read_to_string(root.join("version.txt")).unwrap(), "new");
        assert!(!old.exists() && !fresh.exists());
        // Новой папки нет — старая остаётся на месте.
        assert!(swap_dirs(&root, &fresh, &old).is_err());
        assert_eq!(std::fs::read_to_string(root.join("version.txt")).unwrap(), "new");
        let _ = std::fs::remove_dir_all(&base);
    }
}

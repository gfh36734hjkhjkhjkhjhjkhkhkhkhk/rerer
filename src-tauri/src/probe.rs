//! Проверка, открываются ли сервисы.

use std::time::{Duration, Instant};

use futures_util::future::join_all;
use serde::Serialize;

#[derive(Clone, Serialize, Default)]
pub struct Check {
    pub ok: bool,
    pub ms: u64,
    /// Сколько адресов из проверенных ответили. Для YouTube это важно:
    /// страница может открываться, а видео с googlevideo.com — нет.
    pub good: u8,
    pub total: u8,
    /// Какие адреса не ответили: видно, что именно режет провайдер.
    pub failed: Vec<&'static str>,
}

/// Адреса проверки: (что это, адрес).
type Targets = [(&'static str, &'static str)];

const DISCORD: &Targets = &[
    ("api", "https://discord.com/api/v9/experiments"),
    ("gateway", "https://gateway.discord.gg/?v=10&encoding=json"),
];

/// Для YouTube проверяем и страницу, и сам видеосервер: на части провайдеров
/// сайт открывается, а видео не грузится.
const YOUTUBE: &Targets = &[
    ("сайт", "https://www.youtube.com/generate_204"),
    ("видеосервер", "https://redirector.googlevideo.com/report_mapping?di=no"),
    ("api", "https://youtubei.googleapis.com/youtubei/v1/visitor_id"),
];

pub fn client(timeout: u64) -> reqwest::Client {
    reqwest::Client::builder()
        .timeout(Duration::from_secs(timeout))
        .connect_timeout(Duration::from_secs(timeout))
        // Без этого Google отдаёт 429 и редирект на страницу согласия.
        .user_agent("Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/140.0.0.0 Safari/537.36")
        .redirect(reqwest::redirect::Policy::limited(3))
        .no_proxy()
        .build()
        .expect("http client")
}

/// Время ответа адреса или None. Подходит любой HTTP-ответ: DPI чаще режет
/// соединение или молчит, чем отдаёт ошибку по существу.
async fn http(c: &reqwest::Client, url: &str) -> Option<u64> {
    let t = Instant::now();
    let r = c.get(url).send().await.ok()?;
    // Читаем тело: DPI часто рвёт соединение уже после заголовков.
    r.bytes().await.ok()?;
    Some(t.elapsed().as_millis() as u64)
}

/// Сервис открывается, если ответил хотя бы один из адресов. Адреса проверяются
/// одновременно, поэтому проверка длится не дольше тайм-аута одного запроса.
async fn any(c: &reqwest::Client, targets: &Targets) -> Check {
    let results = join_all(targets.iter().map(|(_, url)| http(c, url))).await;
    summarize(targets, &results)
}

fn summarize(targets: &Targets, results: &[Option<u64>]) -> Check {
    let good = results.iter().flatten().count();
    Check {
        ok: good > 0,
        ms: results.iter().flatten().copied().min().unwrap_or(0),
        good: good as u8,
        total: targets.len() as u8,
        failed: targets.iter().zip(results).filter(|(_, r)| r.is_none()).map(|((name, _), _)| *name).collect(),
    }
}

pub async fn tcp(port: u16) -> Check {
    let t = Instant::now();
    let r = tokio::time::timeout(Duration::from_secs(2), tokio::net::TcpStream::connect(("127.0.0.1", port))).await;
    Check { ok: matches!(r, Ok(Ok(_))), ms: t.elapsed().as_millis() as u64, ..Default::default() }
}

pub struct Services {
    pub discord: Check,
    pub youtube: Check,
}

pub async fn services(timeout: u64) -> Services {
    let c = client(timeout);
    let (discord, youtube) = tokio::join!(any(&c, DISCORD), any(&c, YOUTUBE));
    Services { discord, youtube }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn summarizes_partial() {
        let c = summarize(YOUTUBE, &[Some(140), None, Some(90)]);
        assert!(c.ok);
        assert_eq!((c.ms, c.good, c.total), (90, 2, 3));
        assert_eq!(c.failed, vec!["видеосервер"]);
    }

    #[test]
    fn summarizes_blocked() {
        let c = summarize(DISCORD, &[None, None]);
        assert!(!c.ok);
        assert_eq!((c.ms, c.good, c.total), (0, 0, 2));
        assert_eq!(c.failed, vec!["api", "gateway"]);
    }
}

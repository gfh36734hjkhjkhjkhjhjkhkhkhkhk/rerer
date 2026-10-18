//! Проверка, открываются ли сервисы.

use std::time::{Duration, Instant};

use serde::Serialize;

#[derive(Clone, Serialize, Default)]
pub struct Check {
    pub ok: bool,
    pub ms: u64,
    /// Сколько сайтов из проверенных ответили. Для YouTube это важно:
    /// страница может открываться, а видео с googlevideo.com — нет.
    pub good: u8,
    pub total: u8,
}

fn client(timeout: u64) -> reqwest::Client {
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

/// Один адрес отвечает, если получили любой HTTP-ответ: DPI чаще режет
/// соединение или молчит, чем отдаёт ошибку по существу.
pub async fn http(c: &reqwest::Client, url: &str) -> Check {
    let t = Instant::now();
    match c.get(url).send().await {
        Ok(r) => {
            // Читаем начало тела: DPI часто рвёт соединение уже после заголовков.
            let ok = r.bytes().await.is_ok();
            Check { ok, ms: t.elapsed().as_millis() as u64, ..Default::default() }
        }
        Err(_) => Check::default(),
    }
}

/// Сервис открывается, если ответил хотя бы один из адресов.
/// Для YouTube проверяем и страницу, и сам видеосервер: на части провайдеров
/// сайт открывается, а видео не грузится.
pub async fn any(c: &reqwest::Client, urls: &[&str]) -> Check {
    let mut good = 0u8;
    let mut best = 0u64;
    for u in urls {
        let r = http(c, u).await;
        if r.ok {
            good += 1;
            best = if best == 0 { r.ms } else { best.min(r.ms) };
        }
    }
    let total = urls.len() as u8;
    Check { ok: good > 0, ms: if good > 0 { best } else { 0 }, good, total }
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
    let (discord, youtube) = tokio::join!(
        any(&c, &[
            "https://discord.com/api/v9/experiments",
            "https://gateway.discord.gg/?v=10&encoding=json",
        ]),
        any(&c, &[
            "https://www.youtube.com/generate_204",
            "https://redirector.googlevideo.com/report_mapping?di=no",
            "https://youtubei.googleapis.com/youtubei/v1/visitor_id",
        ]),
    );
    Services { discord, youtube }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn counts_partial() {
        let c = Check { ok: true, ms: 10, good: 2, total: 3 };
        assert!(c.ok && c.good < c.total);
    }
}

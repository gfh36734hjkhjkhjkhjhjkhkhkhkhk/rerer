//! Проверка, открываются ли сервисы.

use std::time::{Duration, Instant};

use serde::Serialize;

#[derive(Clone, Serialize, Default)]
pub struct Check {
    pub ok: bool,
    pub ms: u64,
}

fn client(timeout: u64) -> reqwest::Client {
    reqwest::Client::builder()
        .timeout(Duration::from_secs(timeout))
        .connect_timeout(Duration::from_secs(timeout))
        .redirect(reqwest::redirect::Policy::limited(3))
        .no_proxy()
        .build()
        .expect("http client")
}

pub async fn http(c: &reqwest::Client, url: &str) -> Check {
    let t = Instant::now();
    match c.get(url).send().await {
        Ok(r) => {
            // Читаем начало тела: DPI часто рвёт соединение уже после заголовков.
            let ok = r.bytes().await.is_ok();
            Check { ok, ms: t.elapsed().as_millis() as u64 }
        }
        Err(_) => Check { ok: false, ms: 0 },
    }
}

pub async fn tcp(port: u16) -> Check {
    let t = Instant::now();
    let r = tokio::time::timeout(Duration::from_secs(2), tokio::net::TcpStream::connect(("127.0.0.1", port))).await;
    Check { ok: matches!(r, Ok(Ok(_))), ms: t.elapsed().as_millis() as u64 }
}

pub struct Services {
    pub discord: Check,
    pub youtube: Check,
}

pub async fn services(timeout: u64) -> Services {
    let c = client(timeout);
    let (discord, youtube) = tokio::join!(
        http(&c, "https://discord.com/api/v9/experiments"),
        http(&c, "https://redirector.googlevideo.com/report_mapping?di=no"),
    );
    Services { discord, youtube }
}

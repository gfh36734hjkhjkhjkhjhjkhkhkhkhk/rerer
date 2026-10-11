use std::collections::VecDeque;
use std::io::{BufRead, BufReader, Read};
use std::process::Command;
use std::sync::{Arc, Mutex};

#[cfg(windows)]
use std::os::windows::process::CommandExt;

/// Флаг Windows: запускать процесс без окна консоли.
#[cfg(windows)]
const CREATE_NO_WINDOW: u32 = 0x0800_0000;

/// Команда, которая запускается без мигающего окна консоли.
pub fn hidden(program: impl AsRef<std::ffi::OsStr>) -> Command {
    #[allow(unused_mut)]
    let mut cmd = Command::new(program);
    #[cfg(windows)]
    cmd.creation_flags(CREATE_NO_WINDOW);
    cmd
}

/// Запустить системную команду и дождаться её, игнорируя ошибки.
pub fn run_quiet(program: &str, args: &[&str]) -> bool {
    hidden(program)
        .args(args)
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
}

/// Общий журнал: последние строки вывода движка, прокси и самой программы.
#[derive(Clone, Default)]
pub struct Log(Arc<Mutex<VecDeque<String>>>);

impl Log {
    pub fn push(&self, line: impl Into<String>) {
        let mut q = self.0.lock().unwrap();
        q.push_back(line.into());
        while q.len() > 400 {
            q.pop_front();
        }
    }

    pub fn text(&self) -> String {
        self.0.lock().unwrap().iter().cloned().collect::<Vec<_>>().join("\n")
    }

    /// Читать поток процесса построчно в журнал (в отдельном потоке).
    pub fn pipe<R: Read + Send + 'static>(&self, tag: &'static str, reader: R) {
        let log = self.clone();
        std::thread::spawn(move || {
            for line in BufReader::new(reader).lines().map_while(Result::ok) {
                let line = line.trim_end();
                if !line.is_empty() {
                    log.push(format!("[{tag}] {line}"));
                }
            }
        });
    }
}

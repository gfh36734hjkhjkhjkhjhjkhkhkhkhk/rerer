use std::collections::VecDeque;
use std::fs::{self, File, OpenOptions};
use std::io::{BufRead, BufReader, Read, Write};
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::{Arc, Mutex};

#[cfg(windows)]
use std::os::windows::process::CommandExt;

/// Флаг Windows: запускать процесс без окна консоли.
#[cfg(windows)]
const CREATE_NO_WINDOW: u32 = 0x0800_0000;

/// Сколько строк журнала держим в памяти для окна настроек.
const LOG_LINES: usize = 400;
/// Когда prism.log дорастает до этого размера, он становится prism.old.log.
const LOG_FILE_LIMIT: u64 = 1024 * 1024;

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

/// Запустить системную команду и вернуть код выхода и вывод (None, если она не запустилась).
/// Вывод консольных команд Windows приходит в кодировке OEM, поэтому разбирать
/// стоит только латиницу: имена служб, файлов и значения реестра.
pub fn run_output(program: &str, args: &[&str]) -> Option<(i32, String)> {
    let o = hidden(program).args(args).output().ok()?;
    Some((o.status.code().unwrap_or(-1), String::from_utf8_lossy(&o.stdout).into_owned()))
}

#[derive(Default)]
struct LogState {
    lines: VecDeque<String>,
    file: Option<File>,
    path: Option<PathBuf>,
    written: u64,
}

impl LogState {
    fn write_file(&mut self, line: &str) {
        let Some(f) = self.file.as_mut() else { return };
        let row = format!("{line}\r\n");
        if f.write_all(row.as_bytes()).is_ok() {
            self.written += row.len() as u64;
        }
        if self.written > LOG_FILE_LIMIT {
            if let Some(path) = self.path.clone() {
                // Файл закрываем до переименования в prism.old.log.
                drop(self.file.take());
                self.file = open_log(&path);
                self.written = 0;
            }
        }
    }
}

/// Открыть prism.log для дозаписи, отложив слишком большой старый журнал в prism.old.log.
fn open_log(path: &Path) -> Option<File> {
    if fs::metadata(path).map(|m| m.len() > LOG_FILE_LIMIT).unwrap_or(false) {
        let _ = fs::rename(path, path.with_extension("old.log"));
    }
    OpenOptions::new().create(true).append(true).open(path).ok()
}

/// Общий журнал: последние строки вывода движка, прокси и самой программы.
/// Каждая строка с отметкой времени. Журнал дублируется в файл, чтобы причину сбоя
/// можно было найти и после перезапуска, например если Prism запускался вместе с Windows.
#[derive(Clone, Default)]
pub struct Log(Arc<Mutex<LogState>>);

impl Log {
    /// Начать писать журнал в файл. Строки, появившиеся раньше, тоже попадут в него.
    pub fn attach_file(&self, path: &Path) {
        let mut st = self.0.lock().unwrap();
        st.file = open_log(path);
        // Учитываем записанное в прошлые запуски, иначе файл перерастёт LOG_FILE_LIMIT.
        st.written = fs::metadata(path).map(|m| m.len()).unwrap_or(0);
        st.path = Some(path.to_path_buf());
        let day = chrono::Local::now().format("%Y-%m-%d").to_string();
        let early: Vec<String> = st.lines.iter().map(|l| format!("{day} {l}")).collect();
        for l in early {
            st.write_file(&l);
        }
    }

    pub fn push(&self, line: impl Into<String>) {
        let now = chrono::Local::now();
        let line = format!("{} {}", now.format("%H:%M:%S"), line.into());
        let mut st = self.0.lock().unwrap();
        st.write_file(&format!("{} {line}", now.format("%Y-%m-%d")));
        st.lines.push_back(line);
        while st.lines.len() > LOG_LINES {
            st.lines.pop_front();
        }
    }

    pub fn text(&self) -> String {
        self.0.lock().unwrap().lines.iter().cloned().collect::<Vec<_>>().join("\n")
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn log_goes_to_file_with_time() {
        let dir = std::env::temp_dir().join(format!("prism-log-test-{}", std::process::id()));
        let _ = fs::create_dir_all(&dir);
        let path = dir.join("prism.log");
        let log = Log::default();
        log.push("[prism] до открытия файла");
        log.attach_file(&path);
        log.push("[prism] после");
        let body = fs::read_to_string(&path).unwrap();
        assert!(body.contains("[prism] до открытия файла\r\n"));
        assert!(body.contains("[prism] после\r\n"));
        // Строка в окне: «ЧЧ:ММ:СС текст», в файле ещё и дата.
        let shown = log.text();
        let first = shown.lines().next().unwrap();
        assert_eq!(first.as_bytes()[2], b':');
        assert!(first.ends_with("[prism] до открытия файла"));
        assert_eq!(body.lines().next().unwrap().as_bytes()[4], b'-');
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn rotates_log_left_from_last_run() {
        let dir = std::env::temp_dir().join(format!("prism-log-rotate-test-{}", std::process::id()));
        let _ = fs::create_dir_all(&dir);
        let path = dir.join("prism.log");
        fs::write(&path, vec![b'x'; LOG_FILE_LIMIT as usize - 10]).unwrap();
        let log = Log::default();
        log.attach_file(&path);
        log.push("[prism] после этой строки журнал переезжает в prism.old.log");
        assert!(dir.join("prism.old.log").exists());
        assert!(fs::metadata(&path).unwrap().len() < 1024);
        let _ = fs::remove_dir_all(&dir);
    }
}

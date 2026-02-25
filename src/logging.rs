use anyhow::{Result, anyhow};
use chrono::Local;
use log::{LevelFilter, Log, Metadata, Record};
use std::collections::VecDeque;
use std::fs::{File, OpenOptions};
use std::io::Write;
use std::path::PathBuf;
use std::sync::{Mutex, OnceLock};

const MAX_LOGS: usize = 10_000;

struct MemoryLogger {
    entries: Mutex<VecDeque<String>>,
    stream_file: Mutex<Option<File>>,
}

impl MemoryLogger {
    fn new() -> Self {
        Self {
            entries: Mutex::new(VecDeque::with_capacity(MAX_LOGS)),
            stream_file: Mutex::new(None),
        }
    }

    fn push(&self, entry: String) {
        let mut entries = self.entries.lock().unwrap();
        if entries.len() >= MAX_LOGS {
            entries.pop_front();
        }
        entries.push_back(entry);
    }

    fn snapshot(&self) -> Vec<String> {
        let entries = self.entries.lock().unwrap();
        entries.iter().cloned().collect()
    }

    fn set_stream_file(&self, file: File) {
        let mut stream_file = self.stream_file.lock().unwrap();
        *stream_file = Some(file);
    }

    fn write_stream_entry(&self, entry: &str) {
        let mut stream_file = self.stream_file.lock().unwrap();
        if let Some(file) = stream_file.as_mut() {
            let _ = writeln!(file, "{}", entry);
        }
    }
}

impl Log for MemoryLogger {
    fn enabled(&self, _metadata: &Metadata) -> bool {
        true
    }

    fn log(&self, record: &Record) {
        if !self.enabled(record.metadata()) {
            return;
        }

        let timestamp = Local::now().to_rfc3339();
        let file = record.file().unwrap_or("?");
        let line = record
            .line()
            .map(|l| l.to_string())
            .unwrap_or("?".to_string());
        let entry = format!(
            "{} [{}] {}:{} {}: {}",
            timestamp,
            record.level(),
            file,
            line,
            record.target(),
            record.args()
        );
        self.write_stream_entry(&entry);
        self.push(entry);
    }

    fn flush(&self) {}
}

static LOGGER: OnceLock<MemoryLogger> = OnceLock::new();

pub fn init() -> Result<()> {
    let logger = LOGGER.get_or_init(MemoryLogger::new);
    match log::set_logger(logger) {
        Ok(()) => {
            log::set_max_level(LevelFilter::Trace);
            Ok(())
        }
        Err(_) => {
            log::set_max_level(LevelFilter::Trace);
            Ok(())
        }
    }
}

pub fn print_logs() {
    if let Some(logger) = LOGGER.get() {
        let entries = logger.snapshot();
        for entry in entries {
            eprintln!("{}", entry);
        }
    }
}

pub fn dump_logs() -> Result<PathBuf> {
    let logger = LOGGER
        .get()
        .ok_or_else(|| anyhow!("Logger not initialized"))?;
    let pid = unsafe { libc::getpid() };
    let filename = format!("flyline_logs_{}.txt", pid);
    let path = std::env::current_dir()?.join(filename);

    let entries = logger.snapshot();
    let mut file = File::create(&path)?;
    for entry in entries {
        writeln!(file, "{}", entry)?;
    }

    Ok(path)
}

pub fn stream_logs(path: PathBuf) -> Result<PathBuf> {
    let logger = LOGGER
        .get()
        .ok_or_else(|| anyhow!("Logger not initialized"))?;
    let entries = logger.snapshot();

    let mut file = OpenOptions::new().create(true).append(true).open(&path)?;

    for entry in entries {
        writeln!(file, "{}", entry)?;
    }

    logger.set_stream_file(file);

    Ok(path)
}

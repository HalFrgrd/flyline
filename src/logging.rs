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
    stream_writer: Mutex<Option<Box<dyn Write + Send>>>,
}

impl MemoryLogger {
    fn new() -> Self {
        Self {
            entries: Mutex::new(VecDeque::with_capacity(MAX_LOGS)),
            stream_writer: Mutex::new(None),
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

    fn set_stream_writer(&self, writer: Box<dyn Write + Send>) {
        let mut stream_writer = self.stream_writer.lock().unwrap();
        *stream_writer = Some(writer);
    }

    fn write_stream_entry(&self, entry: &str) {
        let mut stream_writer = self.stream_writer.lock().unwrap();
        if let Some(writer) = stream_writer.as_mut() {
            let _ = writeln!(writer, "{}", entry);
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

/// A writer wrapper that converts `\n` to `\r\n` for use when the terminal is
/// in raw mode, where bare newlines do not return the cursor to column zero.
struct RawModeWriter {
    inner: Box<dyn Write + Send>,
}

impl Write for RawModeWriter {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        // We use write_all for each segment so that every byte in `buf` is
        // either fully forwarded (possibly expanded to "\r\n") or an error is
        // returned.  Because write_all guarantees all-or-error semantics, it is
        // correct to report buf.len() as the number of bytes consumed on
        // success.
        let mut start = 0;
        for (i, &b) in buf.iter().enumerate() {
            if b == b'\n' {
                if start < i {
                    self.inner.write_all(&buf[start..i])?;
                }
                self.inner.write_all(b"\r\n")?;
                start = i + 1;
            }
        }
        if start < buf.len() {
            self.inner.write_all(&buf[start..])?;
        }
        Ok(buf.len())
    }

    fn flush(&mut self) -> std::io::Result<()> {
        self.inner.flush()
    }
}

pub fn stream_logs(path: PathBuf) -> Result<PathBuf> {
    let logger = LOGGER
        .get()
        .ok_or_else(|| anyhow!("Logger not initialized"))?;
    let entries = logger.snapshot();

    let mut writer: Box<dyn Write + Send> = if path.as_os_str() == "stderr" {
        Box::new(RawModeWriter {
            inner: Box::new(std::io::stderr()),
        })
    } else {
        let file = OpenOptions::new().create(true).append(true).open(&path)?;
        Box::new(file)
    };

    for entry in entries {
        writeln!(writer, "{}", entry)?;
    }

    logger.set_stream_writer(writer);

    Ok(path)
}

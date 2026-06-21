use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{LazyLock, Mutex};
use std::time::{Duration, Instant};

pub static RECORDING_ACTIVE: AtomicBool = AtomicBool::new(false);

pub static PERF_RECORDER: LazyLock<Mutex<PerfRecorder>> = LazyLock::new(|| {
    Mutex::new(PerfRecorder::new())
});

#[derive(Debug)]
pub struct PerfRecorder {
    records: HashMap<String, Vec<Duration>>,
}

impl PerfRecorder {
    fn new() -> Self {
        Self {
            records: HashMap::new(),
        }
    }

    pub fn record(&mut self, key: &str, duration: Duration) {
        if RECORDING_ACTIVE.load(Ordering::Relaxed) {
            self.records.entry(key.to_string()).or_default().push(duration);
        }
    }

    pub fn clear(&mut self) {
        self.records.clear();
    }

    pub fn dump_stdout(&self) {
        let mut report = serde_json::json!({});
        for (key, values) in &self.records {
            if values.is_empty() {
                continue;
            }
            let mut sorted = values.clone();
            sorted.sort();
            let total: Duration = sorted.iter().sum();
            let count = sorted.len();
            let avg = total / count as u32;
            let min = sorted[0];
            let max = sorted[count - 1];
            let p50 = sorted[count / 2];
            let p90 = sorted[(count * 9) / 10];
            let p99 = sorted[(count * 99) / 100];

            report[key] = serde_json::json!({
                "count": count,
                "total_ms": total.as_secs_f64() * 1000.0,
                "avg_ms": avg.as_secs_f64() * 1000.0,
                "min_ms": min.as_secs_f64() * 1000.0,
                "max_ms": max.as_secs_f64() * 1000.0,
                "p50_ms": p50.as_secs_f64() * 1000.0,
                "p90_ms": p90.as_secs_f64() * 1000.0,
                "p99_ms": p99.as_secs_f64() * 1000.0,
            });
        }

        if let Ok(json_str) = serde_json::to_string_pretty(&report) {
            println!("{}", json_str);
        }
    }
}

pub fn start_recording() {
    if let Ok(mut recorder) = PERF_RECORDER.lock() {
        recorder.clear();
    }
    RECORDING_ACTIVE.store(true, Ordering::Relaxed);
}

pub fn stop_recording() {
    RECORDING_ACTIVE.store(false, Ordering::Relaxed);
}

pub fn dump_to_stdout() {
    if let Ok(recorder) = PERF_RECORDER.lock() {
        recorder.dump_stdout();
    }
}

pub struct PerfTimer {
    key: &'static str,
    start: Instant,
}

impl PerfTimer {
    pub fn start(key: &'static str) -> Self {
        Self {
            key,
            start: Instant::now(),
        }
    }
}

impl Drop for PerfTimer {
    fn drop(&mut self) {
        let elapsed = self.start.elapsed();
        if let Ok(mut recorder) = PERF_RECORDER.lock() {
            recorder.record(self.key, elapsed);
        }
    }
}



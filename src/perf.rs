use std::collections::HashMap;
use std::fs::File;
use std::io::Write;
use std::sync::{LazyLock, Mutex};
use std::time::{Duration, Instant};

pub static PERF_RECORDER: LazyLock<Mutex<PerfRecorder>> = LazyLock::new(|| {
    let output_path = std::env::var("FLYLINE_PERF_STATS").ok();
    Mutex::new(PerfRecorder::new(output_path))
});

#[derive(Debug)]
pub struct PerfRecorder {
    output_path: Option<String>,
    records: HashMap<String, Vec<Duration>>,
}

impl PerfRecorder {
    fn new(output_path: Option<String>) -> Self {
        Self {
            output_path,
            records: HashMap::new(),
        }
    }

    pub fn record(&mut self, key: &str, duration: Duration) {
        if self.output_path.is_some() {
            self.records.entry(key.to_string()).or_default().push(duration);
        }
    }

    pub fn dump(&self) {
        let Some(ref path) = self.output_path else {
            return;
        };

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

        if let Ok(mut file) = File::create(path) {
            if let Ok(json_str) = serde_json::to_string_pretty(&report) {
                let _ = file.write_all(json_str.as_bytes());
            }
        }
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

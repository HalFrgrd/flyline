use std::process::Command;
use std::time::{SystemTime, UNIX_EPOCH};

fn main() {
    // Capture git commit hash
    let git_hash = Command::new("git")
        .args(["rev-parse", "--short", "HEAD"])
        .output()
        .ok()
        .and_then(|o| String::from_utf8(o.stdout).ok())
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| "unknown".to_string());

    // Capture build datetime (UTC, ISO 8601) — pure std, no extra dependencies
    let build_time = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| format_utc(d.as_secs()))
        .unwrap_or_else(|_| "unknown".to_string());

    println!("cargo:rustc-env=GIT_HASH={git_hash}");
    println!("cargo:rustc-env=BUILD_TIME={build_time}");

    // Re-run when HEAD changes (branch switch or detached-HEAD commit)
    println!("cargo:rerun-if-changed=.git/HEAD");
    // Re-run when the current branch ref changes (new commit on a branch)
    if let Ok(head) = std::fs::read_to_string(".git/HEAD") {
        if let Some(refpath) = head.strip_prefix("ref: ") {
            println!("cargo:rerun-if-changed=.git/{}", refpath.trim());
        }
    }
}

/// Format seconds since Unix epoch as a UTC datetime string (ISO 8601).
fn format_utc(secs: u64) -> String {
    let days = (secs / 86400) as i64;
    let time = secs % 86400;
    let (h, m, s) = (time / 3600, (time % 3600) / 60, time % 60);
    let (year, month, day) = civil_from_days(days);
    format!("{year:04}-{month:02}-{day:02}T{h:02}:{m:02}:{s:02}Z")
}

/// Convert days since the Unix epoch (1970-01-01) to a Gregorian (year, month, day) tuple.
///
/// Algorithm: <https://howardhinnant.github.io/date_algorithms.html>
fn civil_from_days(z: i64) -> (i32, u32, u32) {
    let z = z + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097) as u32;
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = yoe as i64 + era * 400 + i64::from(m <= 2);
    (y as i32, m, d)
}

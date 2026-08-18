use std::collections::HashSet;
use std::fs::File;
use std::io::{BufRead, BufReader, Read};
use std::path::{Path, PathBuf};
use std::process::Command;

use super::backend::{
    HistoryJsonlEvent, append_jsonl_history_events, fetch_flyline_jsonl_history_from_offset,
};
use super::{HistoryEntry, HistoryManager, TimestampNanos};

fn is_sqlite_db_file(path: &Path) -> bool {
    if let Ok(mut file) = File::open(path) {
        let mut header = [0u8; 16];
        if file.read_exact(&mut header).is_ok() {
            return &header == b"SQLite format 3\0";
        }
    }
    false
}

fn load_existing_jsonl_dedup_set(target_jsonl_path: &Path) -> HashSet<(u64, String)> {
    fetch_flyline_jsonl_history_from_offset(target_jsonl_path, None)
        .map(|res| {
            res.events
                .into_iter()
                .filter_map(|event| match event {
                    HistoryJsonlEvent::Start {
                        timestamp, command, ..
                    } => Some((timestamp.as_seconds(), command)),
                    HistoryJsonlEvent::End { .. } => None,
                })
                .collect()
        })
        .unwrap_or_default()
}

fn append_imported_entry_to_jsonl(
    target_jsonl_path: &Path,
    seen_set: &mut HashSet<(u64, String)>,
    entry: &HistoryEntry,
) -> anyhow::Result<bool> {
    if entry.command.trim().is_empty() {
        return Ok(false);
    }
    let ts_secs = entry.timestamp.map(|t| t.as_seconds()).unwrap_or(0);
    if seen_set.contains(&(ts_secs, entry.command.clone())) {
        return Ok(false);
    }
    seen_set.insert((ts_secs, entry.command.clone()));

    let default_session = uuid::Uuid::now_v7().to_string();
    let session_id = entry.session().unwrap_or(&default_session);

    let start_event = entry.to_jsonl_start_event(session_id, None);
    if let Some(end_event) = entry.to_jsonl_end_event() {
        append_jsonl_history_events(&[start_event, end_event], target_jsonl_path)?;
    } else {
        append_jsonl_history_events(&[start_event], target_jsonl_path)?;
    }

    Ok(true)
}

fn import_atuin_sqlite_file(sqlite_path: &Path, target_jsonl_path: &Path) -> anyhow::Result<usize> {
    let mut seen_set = load_existing_jsonl_dedup_set(target_jsonl_path);

    let py_script = r#"
import sqlite3, sys, json

db_path = sys.argv[1]
try:
    conn = sqlite3.connect(db_path)
    cursor = conn.cursor()
    query = """
    SELECT id, timestamp, duration, exit, command, cwd, session, hostname
    FROM history
    WHERE deleted_at IS NULL
    ORDER BY timestamp ASC
    """
    rows = cursor.execute(query).fetchall()
    for row in rows:
        id_str, ts, dur, exit_code, cmd, cwd, session, host = row
        record = {
            "id": str(id_str) if id_str else "",
            "timestamp": int(ts) if ts is not None else 0,
            "duration": int(dur) if dur is not None and dur >= 0 else None,
            "exit": int(exit_code) if exit_code is not None and exit_code >= 0 else None,
            "command": str(cmd) if cmd else "",
            "cwd": str(cwd) if cwd else "",
            "session": str(session) if session else "",
            "hostname": str(host) if host else ""
        }
        sys.stdout.write(json.dumps(record) + "\n")
except Exception:
    sys.exit(1)
"#;

    let mut cmd = Command::new("python3");
    cmd.args(["-c", py_script, sqlite_path.to_str().unwrap_or("")]);

    let _sigchld_guard = crate::SigchldGuard::new();
    let output = cmd
        .output()
        .map_err(|e| anyhow::anyhow!("Failed to execute 'python3': {}", e))?;

    if !output.status.success() {
        return Err(anyhow::anyhow!(
            "Failed to read Atuin SQLite database using Python3"
        ));
    }

    #[derive(serde::Deserialize)]
    struct AtuinPyRecord {
        id: String,
        timestamp: u64,
        duration: Option<u64>,
        exit: Option<i32>,
        command: String,
        cwd: String,
        session: String,
        hostname: String,
    }

    let mut imported_count = 0;
    let stdout_reader = std::io::BufReader::new(&output.stdout[..]);

    for line in stdout_reader.lines().map_while(Result::ok) {
        if line.trim().is_empty() {
            continue;
        }
        let py_rec: AtuinPyRecord = match serde_json::from_str(&line) {
            Ok(r) => r,
            Err(_) => continue,
        };

        let timestamp = TimestampNanos::new(py_rec.timestamp);
        let mut entry = HistoryEntry::new(Some(timestamp.raw_nanos()), 0, py_rec.command);
        let meta = entry.metadata_mut();
        if !py_rec.id.is_empty() {
            meta.id = Some(py_rec.id);
        }
        if !py_rec.cwd.is_empty() {
            meta.cwd = Some(py_rec.cwd);
        }
        if !py_rec.hostname.is_empty() {
            meta.hostname = Some(py_rec.hostname);
        }
        if !py_rec.session.is_empty() {
            meta.session = Some(py_rec.session);
        }
        meta.duration_ns = py_rec.duration;
        meta.exit_status = py_rec.exit;

        if append_imported_entry_to_jsonl(target_jsonl_path, &mut seen_set, &entry)? {
            imported_count += 1;
        }
    }

    Ok(imported_count)
}

pub fn import_history_file(path: &Path, target_jsonl_path: &Path) -> anyhow::Result<usize> {
    if is_sqlite_db_file(path) {
        return import_atuin_sqlite_file(path, target_jsonl_path);
    }
    let content = std::fs::read_to_string(path)?;
    let is_zsh = content.lines().any(|l| l.starts_with(": "));
    let mut entries = if is_zsh {
        HistoryManager::parse_zsh_history_str(&content)
    } else {
        HistoryManager::parse_bash_history_str(&content)
    };

    let mut seen_set = load_existing_jsonl_dedup_set(target_jsonl_path);
    let session = uuid::Uuid::now_v7().to_string();
    let mut imported_count = 0;
    entries.sort_by(|a, b| a.sort_key().cmp(&b.sort_key()));

    for mut entry in entries {
        if entry.session().is_none() {
            entry.metadata_mut().session = Some(session.clone());
        }
        if append_imported_entry_to_jsonl(target_jsonl_path, &mut seen_set, &entry)? {
            imported_count += 1;
        }
    }

    Ok(imported_count)
}

pub fn import_atuin_history(target_jsonl_path: &Path) -> anyhow::Result<usize> {
    let db_path = dirs::data_local_dir()
        .unwrap_or_else(|| PathBuf::from("~/.local/share"))
        .join("atuin")
        .join("history.db");

    if !db_path.exists() {
        return Err(anyhow::anyhow!(
            "Atuin database file not found at {}",
            db_path.display()
        ));
    }

    import_atuin_sqlite_file(&db_path, target_jsonl_path)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_import_bash_history_file() {
        let temp_dir =
            std::env::temp_dir().join(format!("flyline_test_hist_{}", rand::random::<u64>()));
        let _ = std::fs::create_dir_all(&temp_dir);
        let hist_file = temp_dir.join("bash_history");
        let target_jsonl = temp_dir.join("history.jsonl");
        std::fs::write(&hist_file, "#1700000000\nls -la\n#1700000010\ncargo test\n").unwrap();

        let count = import_history_file(&hist_file, &target_jsonl).unwrap();
        assert_eq!(count, 2);

        let _ = std::fs::remove_dir_all(&temp_dir);
    }

    #[test]
    fn test_import_zsh_history_file() {
        let temp_dir =
            std::env::temp_dir().join(format!("flyline_test_zsh_hist_{}", rand::random::<u64>()));
        let _ = std::fs::create_dir_all(&temp_dir);
        let hist_file = temp_dir.join("zsh_history");
        let target_jsonl = temp_dir.join("history.jsonl");
        std::fs::write(
            &hist_file,
            ": 1700000000:0;ls -la\n: 1700000010:0;cargo test\n",
        )
        .unwrap();

        let count = import_history_file(&hist_file, &target_jsonl).unwrap();
        assert_eq!(count, 2);

        let _ = std::fs::remove_dir_all(&temp_dir);
    }

    #[test]
    fn test_import_history_idempotent() {
        let temp_dir =
            std::env::temp_dir().join(format!("flyline_test_idempotent_{}", rand::random::<u64>()));
        let _ = std::fs::create_dir_all(&temp_dir);
        let hist_file = temp_dir.join("bash_history_no_ts");
        let target_jsonl = temp_dir.join("history.jsonl");
        std::fs::write(&hist_file, "echo hello\necho world\n").unwrap();

        let count1 = import_history_file(&hist_file, &target_jsonl).unwrap();
        assert_eq!(count1, 2);

        let count2 = import_history_file(&hist_file, &target_jsonl).unwrap();
        assert_eq!(count2, 0);

        let _ = std::fs::remove_dir_all(&temp_dir);
    }

    #[test]
    fn test_import_atuin_history() {
        let temp_dir =
            std::env::temp_dir().join(format!("flyline_test_atuin_hist_{}", rand::random::<u64>()));
        let _ = std::fs::create_dir_all(&temp_dir);
        let target_jsonl = temp_dir.join("history.jsonl");

        let res = import_atuin_history(&target_jsonl);
        if let Ok(count) = res {
            assert!(target_jsonl.exists());
            println!("Imported {} items from Atuin in test", count);
        }

        let _ = std::fs::remove_dir_all(&temp_dir);
    }

    #[test]
    fn test_import_atuin_sqlite_file() {
        let temp_dir = std::env::temp_dir().join(format!(
            "flyline_test_atuin_sqlite_{}",
            rand::random::<u64>()
        ));
        let _ = std::fs::create_dir_all(&temp_dir);
        let db_path = temp_dir.join("history.db");
        let target_jsonl = temp_dir.join("history.jsonl");

        let py_setup = format!(
            r#"
import sqlite3
conn = sqlite3.connect("{}")
conn.execute("CREATE TABLE history (id text primary key, timestamp integer not null, duration integer not null, exit integer not null, command text not null, cwd text not null, session text not null, hostname text not null, deleted_at integer)")
conn.execute("INSERT INTO history VALUES ('id1', 1785102235000000000, 1500000000, 0, 'echo sqlite_test', '/home/user', 'session1', 'host1', NULL)")
conn.commit()
"#,
            db_path.to_str().unwrap()
        );

        let py_status = std::process::Command::new("python3")
            .args(["-c", &py_setup])
            .status();

        if let Ok(status) = py_status
            && status.success()
        {
            let count = import_atuin_sqlite_file(&db_path, &target_jsonl).unwrap();
            assert_eq!(count, 1);
            let content = std::fs::read_to_string(&target_jsonl).unwrap();
            assert!(content.contains("echo sqlite_test"));
            assert!(content.contains("/home/user"));
            assert!(content.contains("host1"));
        }

        let _ = std::fs::remove_dir_all(&temp_dir);
    }
}

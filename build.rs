use std::process::Command;

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

    // Capture build datetime (UTC, ISO 8601) using chrono (already a project dependency)
    let build_time = chrono::Utc::now().format("%Y-%m-%dT%H:%M:%SZ").to_string();

    println!("cargo:rustc-env=GIT_HASH={git_hash}");
    println!("cargo:rustc-env=BUILD_TIME={build_time}");

    // Re-run when HEAD changes (branch switch or detached-HEAD commit)
    println!("cargo:rerun-if-changed=.git/HEAD");
    // Re-run when the example agent mode file changes (embedded via include_str! in agent_mode.rs)
    println!("cargo:rerun-if-changed=examples/agent_mode.sh");
    // Re-run when the current branch ref changes (new commit on a branch)
    if let Ok(head) = std::fs::read_to_string(".git/HEAD")
        && let Some(refpath) = head.strip_prefix("ref: ")
    {
        println!("cargo:rerun-if-changed=.git/{}", refpath.trim());
    }
}

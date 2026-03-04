use std::env;
use std::process::{Command, Stdio};

fn run_bake_target(target: &str) {
    let stream = env::var("RUST_TEST_NOCAPTURE").is_ok();

    let mut command = Command::new("docker");
    command.args(["buildx", "bake", "-f", "docker/docker-bake.hcl", target]);

    if stream {
        let status = command
            .stdout(Stdio::inherit())
            .stderr(Stdio::inherit())
            .status()
            .expect("Failed to execute docker buildx bake");
        assert!(status.success(), "docker buildx bake failed");
    } else {
        let output = command
            .output()
            .expect("Failed to execute docker buildx bake");
        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            panic!("docker buildx bake failed: {stderr}");
        }
    }
}

#[test]
fn tab_completion_tests() {
    run_bake_target("tab-completion-tests");
}

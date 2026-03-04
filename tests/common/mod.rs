use std::env;
use std::process::{Command, Stdio};

pub fn run_bake_target(target: &str) -> anyhow::Result<()> {
    let stream = env::var("RUST_TEST_NOCAPTURE").is_ok();
    let manifest_dir = env::var("CARGO_MANIFEST_DIR").unwrap_or_else(|_| ".".to_string());

    let mut command = Command::new("docker");
    command.current_dir(manifest_dir).args([
        "buildx",
        "bake",
        "-f",
        "docker/docker-bake.hcl",
        target,
    ]);

    if stream {
        let status = command
            .stdout(Stdio::inherit())
            .stderr(Stdio::inherit())
            .status()?;
        if !status.success() {
            anyhow::bail!("docker buildx bake failed");
        }
    } else {
        let output = command.output()?;
        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            anyhow::bail!("docker buildx bake failed: {stderr}");
        }
    }

    Ok(())
}

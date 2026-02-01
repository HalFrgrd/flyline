use anyhow::Result;
use std::env;
use std::process::{Command, Stdio};

fn run_command(cmd: &str, args: Vec<String>) -> Result<()> {
    let stream = env::var("RUST_TEST_NOCAPTURE").is_ok();
    let mut command = Command::new(cmd);
    command.args(&args);
    if stream {
        let status = command
            .stdout(Stdio::inherit())
            .stderr(Stdio::inherit())
            .status()?;
        if !status.success() {
            anyhow::bail!("Command failed");
        }
    } else {
        let output = command.output()?;
        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            anyhow::bail!("Command failed: {}", stderr);
        }
    }
    Ok(())
}

fn run_bash_version_test(bash_version: &str) -> Result<()> {
    println!("Testing Bash version: {}", bash_version);

    // Ensure the builder image reflects current source
    run_command("docker/docker_build.sh", vec![])?;

    run_command(
        "docker",
        vec![
            "build".to_string(),
            "--build-arg".to_string(),
            format!("BASH_VERSION={}", bash_version),
            "--file".to_string(),
            "docker/bash_integration_test.Dockerfile".to_string(),
            "--tag".to_string(),
            format!("flyline-test-bash{}", bash_version.replace(".", "")),
            "docker/build".to_string(),
        ],
    )?;

    println!("Successfully tested Bash {} with flyline", bash_version);
    Ok(())
}


#[test]
fn test_bash_4_4_30() {
    run_bash_version_test("4.4.30").expect("Bash 4.4.30 integration test failed");
}

#[test]
fn test_bash_5_1_16() {
    run_bash_version_test("5.1.16").expect("Bash 5.1.16 integration test failed");
}
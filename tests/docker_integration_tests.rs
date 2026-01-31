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

fn run_ubuntu_version_test(ubuntu_version: &str) -> Result<()> {
    println!("Testing Ubuntu version: {}", ubuntu_version);

    // Ensure the builder image reflects current source

    run_command("docker/docker_build.sh", vec![])?;

    // Build the Docker image first using docker command
    run_command(
        "docker",
        vec![
            "build".to_string(),
            "--target".to_string(),
            "ubuntu_testing".to_string(),
            "--build-arg".to_string(),
            format!("UBUNTU_VERSION={}", ubuntu_version),
            "--file".to_string(),
            "docker/Dockerfile.ubuntu.template".to_string(),
            "--tag".to_string(),
            format!("flyline-test-ubuntu{}", ubuntu_version.replace(".", "")),
            ".".to_string(),
        ],
    )?;

    // Test the built image by running it
    run_command(
        "docker",
        vec![
            "run".to_string(),
            "--rm".to_string(),
            format!("flyline-test-ubuntu{}", ubuntu_version.replace(".", "")),
            // "bash".to_string(),
            // "-lc".to_string(),
            // "flyline -s && flyline -v && echo 'SUCCESS: Test completed'".to_string(),
        ],
    )?;

    println!("Successfully tested Ubuntu {} with flyline", ubuntu_version);
    Ok(())
}

#[test]
fn test_ubuntu_2404() {
    if let Err(e) = run_ubuntu_version_test("24.04") {
        panic!("Ubuntu 24.04 integration test failed: {}", e);
    }
}

#[test]
fn test_ubuntu_2204() {
    if let Err(e) = run_ubuntu_version_test("22.04") {
        panic!("Ubuntu 22.04 integration test failed: {}", e);
    }
}

#[test]
fn test_ubuntu_2004() {
    if let Err(e) = run_ubuntu_version_test("20.04") {
        panic!("Ubuntu 20.04 integration test failed: {}", e);
    }
}

#[test]
fn test_ubuntu_1804() {
    if let Err(e) = run_ubuntu_version_test("18.04") {
        panic!("Ubuntu 18.04 integration test failed: {}", e);
    }
}

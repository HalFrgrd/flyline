use anyhow::Result;
use std::env;
use std::process::{Command, Stdio};

fn is_gha() -> bool {
    env::var("GITHUB_ACTIONS")
        .map(|v| v == "true")
        .unwrap_or(false)
}

fn has_buildx() -> bool {
    Command::new("docker")
        .args(["buildx", "version"])
        .status()
        .map(|s| s.success())
        .unwrap_or(false)
}

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

fn run_docker_build(mut args: Vec<String>, cache_scope: &str) -> Result<()> {
    if is_gha() && has_buildx() {
        let mut bx_args = vec![
            "buildx".to_string(),
            "build".to_string(),
            "--cache-from".to_string(),
            format!("type=gha,scope={}", cache_scope),
            "--cache-to".to_string(),
            format!("type=gha,mode=max,scope={}", cache_scope),
            "--load".to_string(),
        ];
        bx_args.append(&mut args);
        run_command("docker", bx_args)
    } else {
        let mut classic_args = vec!["build".to_string()];
        classic_args.append(&mut args);
        run_command("docker", classic_args)
    }
}

fn run_ubuntu_version_test(ubuntu_version: &str) -> Result<()> {
    println!("Testing Ubuntu version: {}", ubuntu_version);
    // Ensure the builder image reflects current source
    run_docker_build(
        vec![
            "--tag".to_string(),
            "flyline_built_library".to_string(),
            "--file".to_string(),
            "Dockerfile".to_string(),
            "--target".to_string(),
            "flyline_built_library".to_string(),
            ".".to_string(),
        ],
        "builderlib",
    )?;

    // Build the Docker image first using docker command
    run_docker_build(
        vec![
            "--target".to_string(),
            "ubuntu_testing".to_string(),
            "--build-arg".to_string(),
            format!("UBUNTU_VERSION={}", ubuntu_version),
            "-f".to_string(),
            "tests/docker_integration_tests/Dockerfile.ubuntu.template".to_string(),
            "-t".to_string(),
            format!("flyline-test-ubuntu{}", ubuntu_version.replace(".", "")),
            ".".to_string(),
        ],
        &format!("ubuntu-testing-{}", ubuntu_version.replace('.', "")),
    )?;

    // Test the built image by running it
    run_command(
        "docker",
        vec![
            "run".to_string(),
            "--rm".to_string(),
            format!("flyline-test-ubuntu{}", ubuntu_version.replace(".", "")),
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

use anyhow::Result;
use std::process::Command;

fn run_ubuntu_version_test(ubuntu_version: &str) -> Result<()> {
    // Build the Docker image first using docker command
    let build_output = Command::new("docker")
        .args([
            "build",
            "--target",
            "ubuntu_testing",
            "--build-arg",
            &format!("UBUNTU_VERSION={}", ubuntu_version),
            "-f",
            "tests/docker_integration_tests/Dockerfile.ubuntu.template",
            "-t",
            &format!("flyline-test-ubuntu{}", ubuntu_version.replace(".", "")),
            ".",
        ])
        .output()?;

    if !build_output.status.success() {
        let stderr = String::from_utf8_lossy(&build_output.stderr);
        anyhow::bail!("Docker build failed: {}", stderr);
    }

    // Test the built image by running it
    let run_output = Command::new("docker")
        .args([
            "run",
            "--rm",
            &format!("flyline-test-ubuntu{}", ubuntu_version.replace(".", "")),
        ])
        .output()?;

    // Print stdout for debugging
    let stdout = String::from_utf8_lossy(&run_output.stdout);
    println!("Docker run output: {}", stdout);

    if !run_output.status.success() {
        let stderr = String::from_utf8_lossy(&run_output.stderr);
        anyhow::bail!("Docker run failed: {}", stderr);
    }

    println!("Successfully tested Ubuntu {} with flyline", ubuntu_version);
    Ok(())
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

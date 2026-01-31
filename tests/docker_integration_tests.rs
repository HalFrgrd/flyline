use anyhow::Result;
use std::process::Command;
use std::process::Stdio;

fn run_ubuntu_version_test(ubuntu_version: &str) -> Result<()> {
    // Ensure the builder image reflects current source
    let builder_build = Command::new("docker")
        .args(["build", "-t", "flyline-builder", "-f", "Dockerfile", "."])
        .output()?;

    if !builder_build.status.success() {
        let stderr = String::from_utf8_lossy(&builder_build.stderr);
        anyhow::bail!("Builder image build failed: {}", stderr);
    }

    // Build the Docker image first using docker command
    let build_status = Command::new("docker")
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
        .stdout(Stdio::inherit())
        .stderr(Stdio::inherit())
        .status()?;

    if !build_status.success() {
        anyhow::bail!("Docker build failed");
    }

    // Test the built image by running it
    let run_status = Command::new("docker")
        .args([
            "run",
            "--rm",
            &format!("flyline-test-ubuntu{}", ubuntu_version.replace(".", "")),
        ])
        .stdout(Stdio::inherit())
        .stderr(Stdio::inherit())
        .status()?;

    if !run_status.success() {
        anyhow::bail!("Docker run failed");
    }

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

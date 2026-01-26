use std::process::Command;
use std::env;
use anyhow::Result;


fn handle_command_output(output: &std::process::Output) -> Result<()> {
    if output.status.success() {
        Ok(())
    } else {
        let stderr = String::from_utf8_lossy(&output.stderr);
        let stdout = String::from_utf8_lossy(&output.stdout);
        Err(anyhow::anyhow!(format!(
            "Command failed:\nSTDOUT:\n{}\nSTDERR:\n{}",
            stdout, stderr
        )))
    }
}

fn run_integration_test(test_name: &str) -> Result<()> {
    let project_root = env!("CARGO_MANIFEST_DIR");
    
    // Run the specific integration test
    let dockerfile = format!("tests/docker_integration_tests/Dockerfile.{}", test_name);
    let image_tag = format!("flyline-test-{}", test_name);

    // Build the image from Dockerfile.<test_name>
    let build_output = Command::new("docker")
        .args(&[
            "build", 
            "-f", &dockerfile, 
            "-t", &image_tag,
            "."
        ])
        .current_dir(&project_root)
        .output()
        .map_err(|e| anyhow::anyhow!(format!("Failed to execute docker build: {}", e)))?;

    handle_command_output(&build_output)?;

    // Run the built image
    let output = Command::new("docker")
        .args(&["run", "--rm", &image_tag])
        .output()
        .map_err(|e| anyhow::anyhow!(format!("Failed to execute docker run: {}", e)))?;

    handle_command_output(&output)
}

fn check_docker_available() -> Result<()> {
    // Check if Docker is available
    let output = Command::new("docker")
        .args(&["--version"])
        .output()
        .map_err(|_| anyhow::anyhow!("Docker is not available. Please install Docker to run integration tests."))?;

    if !output.status.success() {
        return Err(anyhow::anyhow!("Docker is not working properly."));
    }

    // Check if Docker daemon is running
    let output = Command::new("docker")
        .args(&["info"])
        .output()
        .map_err(|_| anyhow::anyhow!("Failed to check Docker daemon status."))?;

    if !output.status.success() {
        return Err(anyhow::anyhow!("Docker daemon is not running. Please start Docker."));
    }

    Ok(())
}

#[test]
fn test_docker_available() {
    if let Err(e) = check_docker_available() {
        panic!("Docker prerequisite check failed: {}", e);
    }
}


#[test]
fn test_bash_latest_ubuntu2204() {
    if let Err(e) = check_docker_available() {
        eprintln!("Skipping Docker test - Docker not available: {}", e);
        return;
    }

    if let Err(e) = run_integration_test("bash_latest") {
        panic!("Latest Bash (Ubuntu 22.04) integration test failed: {}", e);
    }
}


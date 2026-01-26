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

fn run_ubuntu_version_test(ubuntu_version: &str) -> Result<()> {
    let project_root = env!("CARGO_MANIFEST_DIR");
    
    // Step 1: Build the project using Dockerfile.glibc231 to get the shared library
    let build_image_tag = "flyline-builder-glibc231";
    let build_output = Command::new("docker")
        .args(&[
            "build", 
            "-f", "Dockerfile.glibc231", 
            "-t", build_image_tag,
            "."
        ])
        .current_dir(&project_root)
        .output()
        .map_err(|e| anyhow::anyhow!(format!("Failed to execute docker build for glibc231: {}", e)))?;

    handle_command_output(&build_output)?;

    // Step 2: Extract the shared library from the builder container
    let extract_output = Command::new("docker")
        .args(&[
            "run", "--rm", 
            "-v", &format!("{}:/host", project_root),
            build_image_tag,
            "cp", "/workspace/target/release/libflyline.so", "/host/libflyline-glibc231.so"
        ])
        .current_dir(&project_root)
        .output()
        .map_err(|e| anyhow::anyhow!(format!("Failed to extract shared library: {}", e)))?;

    handle_command_output(&extract_output)?;

    // Step 3: Build the test image using the template with Ubuntu version
    let test_image_tag = format!("flyline-test-ubuntu{}", ubuntu_version.replace(".", ""));
    
    let test_build_output = Command::new("docker")
        .args(&[
            "build", 
            "--build-arg", &format!("UBUNTU_VERSION={}", ubuntu_version),
            "-f", "tests/docker_integration_tests/Dockerfile.ubuntu.template", 
            "-t", &test_image_tag,
            "."
        ])
        .current_dir(&project_root)
        .output()
        .map_err(|e| anyhow::anyhow!(format!("Failed to execute docker build for Ubuntu {}: {}", ubuntu_version, e)))?;

    handle_command_output(&test_build_output)?;

    // Step 4: Run the test container
    let output = Command::new("docker")
        .args(&["run", "--rm", &test_image_tag])
        .output()
        .map_err(|e| anyhow::anyhow!(format!("Failed to execute docker run for Ubuntu {}: {}", ubuntu_version, e)))?;

    handle_command_output(&output)
}

fn run_integration_test(test_name: &str) -> Result<()> {
    let project_root = env!("CARGO_MANIFEST_DIR");
    
    // Step 1: Build the project using Dockerfile.glibc231 to get the shared library
    let build_image_tag = "flyline-builder-glibc231";
    let build_output = Command::new("docker")
        .args(&[
            "build", 
            "-f", "Dockerfile.glibc231", 
            "-t", build_image_tag,
            "."
        ])
        .current_dir(&project_root)
        .output()
        .map_err(|e| anyhow::anyhow!(format!("Failed to execute docker build for glibc231: {}", e)))?;

    handle_command_output(&build_output)?;

    // Step 2: Extract the shared library from the builder container
    let extract_output = Command::new("docker")
        .args(&[
            "run", "--rm", 
            "-v", &format!("{}:/host", project_root),
            build_image_tag,
            "cp", "/workspace/target/release/libflyline.so", "/host/libflyline-glibc231.so"
        ])
        .current_dir(&project_root)
        .output()
        .map_err(|e| anyhow::anyhow!(format!("Failed to extract shared library: {}", e)))?;

    handle_command_output(&extract_output)?;

    // Step 3: Build the test image using the specific test Dockerfile
    let dockerfile = format!("tests/docker_integration_tests/Dockerfile.{}", test_name);
    let test_image_tag = format!("flyline-test-{}", test_name);

    let test_build_output = Command::new("docker")
        .args(&[
            "build", 
            "-f", &dockerfile, 
            "-t", &test_image_tag,
            "."
        ])
        .current_dir(&project_root)
        .output()
        .map_err(|e| anyhow::anyhow!(format!("Failed to execute docker build for test {}: {}", test_name, e)))?;

    handle_command_output(&test_build_output)?;

    // Step 4: Run the test container
    let output = Command::new("docker")
        .args(&["run", "--rm", &test_image_tag])
        .output()
        .map_err(|e| anyhow::anyhow!(format!("Failed to execute docker run for test {}: {}", test_name, e)))?;

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

#[test]
fn test_ubuntu_2204() {
    if let Err(e) = check_docker_available() {
        eprintln!("Skipping Docker test - Docker not available: {}", e);
        return;
    }

    if let Err(e) = run_ubuntu_version_test("22.04") {
        panic!("Ubuntu 22.04 integration test failed: {}", e);
    }
}

#[test]
fn test_ubuntu_2004() {
    if let Err(e) = check_docker_available() {
        eprintln!("Skipping Docker test - Docker not available: {}", e);
        return;
    }

    if let Err(e) = run_ubuntu_version_test("20.04") {
        panic!("Ubuntu 20.04 integration test failed: {}", e);
    }
}

#[test]
fn test_ubuntu_1804() {
    if let Err(e) = check_docker_available() {
        eprintln!("Skipping Docker test - Docker not available: {}", e);
        return;
    }

    if let Err(e) = run_ubuntu_version_test("18.04") {
        panic!("Ubuntu 18.04 integration test failed: {}", e);
    }
}


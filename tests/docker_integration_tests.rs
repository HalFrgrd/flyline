use std::process::Command;
use std::env;
use anyhow::{Result, Context};


fn handle_command_output(output: &std::process::Output) -> Result<()> {
    if output.status.success() {
        Ok(())
    } else {
        let stderr = String::from_utf8_lossy(&output.stderr);
        let stdout = String::from_utf8_lossy(&output.stdout);
        anyhow::bail!(
            "Command failed with exit code {}:\nSTDOUT:\n{}\nSTDERR:\n{}", 
            output.status.code().unwrap_or(-1),
            stdout, 
            stderr
        );
    }
}

fn run_ubuntu_version_test(ubuntu_version: &str) -> Result<()> {
    let project_root = env!("CARGO_MANIFEST_DIR");
    
    // Step 1: Build the project using Dockerfile.glibc227 to get the shared library
    let build_image_tag = "flyline-builder-glibc227";
    let build_output = Command::new("docker")
        .args(&[
            "build", 
            "-f", "Dockerfile.glibc227", 
            "-t", build_image_tag,
            "."
        ])
        .current_dir(&project_root)
        .output()
        .with_context(|| "Failed to execute docker build for glibc227")?;

    handle_command_output(&build_output)
        .with_context(|| "Docker build failed")?;

    // Step 2: Extract the shared library from the builder container using docker cp (cleaner approach)
    let extract_library_name = "libflyline-glibc227.so";
    let container_name = format!("flyline-extract-{}", std::process::id());
    
    // Create a temporary container to extract from
    let create_output = Command::new("docker")
        .args(&[
            "create", 
            "--name", &container_name,
            build_image_tag
        ])
        .current_dir(&project_root)
        .output()
        .with_context(|| format!("Failed to create temporary container: {}", container_name))?;

    handle_command_output(&create_output)
        .with_context(|| "Failed to create extraction container")?;

    // Copy the file from the container
    let extract_output = Command::new("docker")
        .args(&[
            "cp", 
            &format!("{}:/workspace/target/release/libflyline.so", container_name),
            &format!("./{}", extract_library_name)
        ])
        .current_dir(&project_root)
        .output()
        .with_context(|| "Failed to copy shared library from container")?;

    // Clean up the temporary container (ignore errors)
    let _cleanup = Command::new("docker")
        .args(&["rm", &container_name])
        .current_dir(&project_root)
        .output();

    handle_command_output(&extract_output)
        .with_context(|| format!("Failed to extract {}", extract_library_name))?;

    // Step 3: Build the test image using the template with Ubuntu version
    let test_image_tag = format!("flyline-test-ubuntu{}", ubuntu_version.replace(".", ""));
    
    let test_build_output = Command::new("docker")
        .args(&[
            "build", 
            "--build-arg", &format!("UBUNTU_VERSION={}", ubuntu_version),
            "--build-arg", &format!("FLYLINE_LIB_PATH={}", extract_library_name),
            "-f", "tests/docker_integration_tests/Dockerfile.ubuntu.template", 
            "-t", &test_image_tag,
            "."
        ])
        .current_dir(&project_root)
        .output()
        .with_context(|| format!("Failed to build test image for Ubuntu {}", ubuntu_version))?;

    handle_command_output(&test_build_output)
        .with_context(|| format!("Test image build failed for Ubuntu {}", ubuntu_version))?;

    // Step 4: Run the test container
    let output = Command::new("docker")
        .args(&["run", "--rm", &test_image_tag])
        .output()
        .with_context(|| format!("Failed to run test container for Ubuntu {}", ubuntu_version))?;

    handle_command_output(&output)
        .with_context(|| format!("Test execution failed for Ubuntu {}", ubuntu_version))
}

fn check_docker_available() -> Result<()> {
    // Check if Docker is available and working
    let output = Command::new("docker")
        .args(&["--version"])
        .output()
        .with_context(|| "Docker is not available. Please install Docker to run integration tests.")?;
    
    anyhow::ensure!(output.status.success(), "Docker is not working properly");

    // Check if Docker daemon is running  
    let output = Command::new("docker")
        .args(&["info"])
        .output()
        .with_context(|| "Failed to check Docker daemon status")?;
    
    anyhow::ensure!(output.status.success(), "Docker daemon is not running. Please start Docker");

    Ok(())
}

#[test]
fn test_docker_available() {
    if let Err(e) = check_docker_available() {
        panic!("Docker prerequisite check failed: {}", e);
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


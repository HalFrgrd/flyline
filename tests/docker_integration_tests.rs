use std::env;
use anyhow::{Result, Context};
use tempfile::tempdir;
use testcontainers::{
    core::BuildImageOptions,
    runners::{SyncBuilder, SyncRunner},
    GenericBuildableImage,
};

fn run_ubuntu_version_test(ubuntu_version: &str) -> Result<()> {
    let project_root = env!("CARGO_MANIFEST_DIR");
    std::env::set_current_dir(&project_root)
        .with_context(|| format!("Failed to change current directory to project root: {}", project_root))?;
    
    // Step 1: Build the project using Dockerfile.glibc227 to get the shared library
    let image = GenericBuildableImage::new("flyline-builder-glibc227", ubuntu_version)
        .with_dockerfile("Dockerfile.glibc227")
        .build_image_with(
            BuildImageOptions::new()
                .with_skip_if_exists(true)
        )?;


    // Step 2: Extract the shared library from the builder container using docker cp (cleaner approach)
    let extract_library_name = "libflyline-glibc227.so";

    let destination = tempdir()?.path().join(extract_library_name);
    println!("Extracting built library to {:?}", destination);
    let _build_container = image.start()?.copy_file_from("/workspace/target/release/libflyline.so", destination.as_path());


    // Step 3: Build and run the test container using testcontainers  
    let test_image = GenericBuildableImage::new("flyline-test", ubuntu_version)
        .with_dockerfile("tests/docker_integration_tests/Dockerfile.ubuntu.template")
        .build_image_with(
            BuildImageOptions::new()
                .with_skip_if_exists(true)
                .with_build_arg("UBUNTU_VERSION", ubuntu_version)
                .with_build_arg("FLYLINE_LIB_PATH", extract_library_name)
        )?;

    // Step 4: Run the test container
    let _test_container = test_image.start()?;
    
    // Container runs the test automatically via CMD in Dockerfile
    // If it starts successfully, the test passed

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


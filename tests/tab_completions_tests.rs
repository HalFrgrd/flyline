use core::panic;
use std::env;
use std::fs;
use std::path::PathBuf;
use std::process::Command;

// cargo build && cargo test --test tab_completions_tests -- --no-capture

#[test]
fn test_tab_completions_integration() {
    // 1. Helper to find the dynamic library
    let manifest_dir = env::var("CARGO_MANIFEST_DIR").expect("CARGO_MANIFEST_DIR not set");
    let debug_lib = PathBuf::from(&manifest_dir).join("target/debug/libflyline.so");
    let release_lib = PathBuf::from(&manifest_dir).join("target/release/libflyline.so");

    let lib_path = if debug_lib.exists() {
        debug_lib
    } else if release_lib.exists() {
        release_lib
    } else {
        eprintln!(
            "Skipping integration test: libflyline.so not found in target/debug or target/release. Please run 'cargo build' first."
        );
        return;
    };

    println!("Using library: {:?}", lib_path);

    // 2. Create a temporary directory
    // We use a specific suffix to easily identify it if it leaks, but ensure uniqueness with PID/time
    let temp_dir = env::temp_dir().join(format!(
        "flyline_integration_test_{}_{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));

    fs::create_dir_all(&temp_dir).expect("Failed to create temp dir");
    println!("Created temp dir: {:?}", temp_dir);

    // fs::create_dir(&temp_dir)
    let example_fs = temp_dir.join("example_fs");
    fs::create_dir(&example_fs).unwrap();

    fs::create_dir(&example_fs.join("foo")).unwrap();
    fs::create_dir(&example_fs.join("many spaces here")).unwrap();
    fs::write(&example_fs.join("file1.txt"), "content").unwrap();
    fs::write(&example_fs.join("file with spaces.txt"), "content").unwrap();

    // Change working directory to temp_dir for the test
    env::set_current_dir(&example_fs).expect("Failed to set working directory to example_fs");

    // 3. Create simple_bashrc.sh
    let bashrc_path = temp_dir.join("simple_bashrc.sh");
    // We also link/copy the completion_util.sh helper if needed?
    // The previous request asked to create it in tests/completion_util.sh.
    // We can source it from there if we know the path.
    let completion_util_path = PathBuf::from(&manifest_dir).join("tests/completion_util.sh");
    let source_util_cmd = if completion_util_path.exists() {
        format!(" \"{}\"", completion_util_path.display())
    } else {
        panic!(
            "Helper script completion_util.sh not found at expected path: {:?}",
            completion_util_path
        );
    };

    let bashrc_content = format!(
        r#"
# Ensure we catch errors? strict mode might be too much for bashrc but helpful
set -e 

if ! shopt -oq posix; then
  if [ -f /usr/share/bash-completion/bash_completion ]; then
    echo "sourcing bash completion"
    . /usr/share/bash-completion/bash_completion
  elif [ -f /etc/bash_completion ]; then
    . /etc/bash_completion
  fi
fi

echo "Loading flyline from {}"
enable -f "{}" flyline

# Source the helper utilities
source {}
flyline --version

# Run the test command provided by flyline
echo "Running flyline --run-tab-completion-tests..."
flyline --run-tab-completion-tests

exit 0
"#,
        lib_path.display(),
        lib_path.display(),
        source_util_cmd
    );

    fs::write(&bashrc_path, bashrc_content).expect("Failed to write simple_bashrc.sh");

    // 4. Run bash in PTY via script
    // We force interactive mode with -i, although running in script (PTY) usually implies it.
    let bash_cmd = format!("bash --rcfile '{}' -i", bashrc_path.display());

    println!("Executing: script -q -c \"{}\" /dev/null", bash_cmd);

    let output = Command::new("script")
        .arg("-q")
        .arg("-c")
        .arg(&bash_cmd)
        .arg("/dev/null")
        // .env_clear() // Maybe clear env to be safe? But we need PATH etc.
        .output()
        .expect("Failed to run script command");

    // 5. Cleanup
    // We try to remove even if it failed
    let _ = fs::remove_dir_all(&temp_dir);

    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);

    println!("--- SCRIPT OUTPUT ---\n{}", stdout);
    println!("--- SCRIPT STDERR ---\n{}", stderr);

    // Verify success
    if !stdout.contains("FLYLINE_TEST_SUCCESS") {
        panic!("Integration test failed. Did not see success marker.");
    }
}

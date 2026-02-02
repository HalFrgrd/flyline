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

macro_rules! bash_integration_test {
    ($name:ident, $version:expr) => {
        #[test]
        fn $name() {
            run_bash_version_test($version).expect(concat!(
                "Bash ",
                $version,
                " integration test failed"
            ));
        }
    };
}

// This one fails because of the lack of builtin_help in Bash 4.3
// #[test]
// bash_integration_test!(test_bash_4_3_30, "4.3.30");

bash_integration_test!(test_bash_4_4_rc1, "4.4-rc1");
bash_integration_test!(test_bash_4_4_18, "4.4.18");
bash_integration_test!(test_bash_5_0, "5.0");
bash_integration_test!(test_bash_5_1_16, "5.1.16");
bash_integration_test!(test_bash_5_2, "5.2");
bash_integration_test!(test_bash_5_3, "5.3");

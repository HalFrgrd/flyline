use anyhow::Result;
use std::env;
use std::process::{Command, Stdio};

fn run_bake_target(target: &str) -> Result<()> {
    let stream = env::var("RUST_TEST_NOCAPTURE").is_ok();
    let mut command = Command::new("docker");
    command.args(["buildx", "bake", "-f", "docker/docker-bake.hcl", target]);
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

fn bake_target_for_bash_version(bash_version: &str) -> String {
    format!("bash-integration-test-{}", bash_version.replace('.', "_"))
}

fn run_bash_version_test(bash_version: &str) -> Result<()> {
    println!("Testing Bash version: {}", bash_version);

    run_bake_target(&bake_target_for_bash_version(bash_version))?;

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

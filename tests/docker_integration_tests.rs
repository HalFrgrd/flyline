mod common;

fn run_bash_version_test(docker_bash_version: &str) {
    println!("Testing Bash version: {}", docker_bash_version);

    let target = format!(
        "bash-integration-test-{}",
        docker_bash_version.replace('.', "_")
    );

    common::run_bake_target(&target).expect(&format!(
        "Test failed for Bash version: {}",
        docker_bash_version
    ));

    println!(
        "Successfully tested Bash {} with flyline",
        docker_bash_version
    );
}

macro_rules! bash_integration_test {
    ($name:ident, $version:expr) => {
        #[test]
        fn $name() {
            run_bash_version_test($version)
        }
    };
}

// This one fails because of the lack of builtin_help in Bash 4.3
// #[test]
// bash_integration_test!(test_bash_4_3_30, "4.3.30");

bash_integration_test!(test_bash_3_2_57, "3.2.57");
bash_integration_test!(test_bash_4_4_rc1, "4.4-rc1");
bash_integration_test!(test_bash_4_4_18, "4.4.18");
bash_integration_test!(test_bash_5_0, "5.0");
bash_integration_test!(test_bash_5_1_16, "5.1.16");
bash_integration_test!(test_bash_5_2, "5.2");
bash_integration_test!(test_bash_5_3, "5.3");

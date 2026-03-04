target "builder" {
    context = "."
    dockerfile = "docker/integration_test_build.Dockerfile"
    target = "flyline-builder"
}

target "built-artifact" {
    context = "."
    dockerfile = "docker/integration_test_build.Dockerfile"
    target = "flyline-built-artifact"
}

target "lib-tests" {
    context = "."
    dockerfile = "docker/integration_test_build.Dockerfile"
    target = "flyline-lib-tests"
}

target "bash-integration-test" {
    context = "."
    contexts = {
        built-artifact = "target:built-artifact"
    }
    name = "bash-integration-test-${replace(bash_version, ".", "_")}"
    matrix = {
        bash_version = ["4.4-rc1", "4.4.18", "5.0", "5.1.16", "5.2", "5.3"]
    }
    dockerfile = "docker/bash_integration_test.Dockerfile"
    args = {
        BASH_VERSION = bash_version
    }
    tags = ["flyline-bash-integration-test:${bash_version}"]
}

target "tab-completion-test" {
    context = "."
    contexts = {
        built-artifact = "target:built-artifact"
    }
    name = "tab-completion-test"
    dockerfile = "docker/tab_completions.Dockerfile"
}

group "default" {
    targets = ["lib-tests"]
}

group "bash-integration-tests" {
    targets = ["bash-integration-test"]
}

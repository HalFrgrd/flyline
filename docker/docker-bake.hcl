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

# example command:
# docker buildx bake -f docker/docker-bake.hcl extract-artifact
target "extract-artifact" {
    context = "."
    output = ["type=local,dest=docker/build"]
    dockerfile = "docker/integration_test_build.Dockerfile"
    target = "flyline-built-artifact"
}

target "lib-tests" {
    context = "."
    dockerfile = "docker/integration_test_build.Dockerfile"
    target = "flyline-lib-tests"
}

target "bash-integration-tests" {
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


target "tab-completion-tests" {
    context = "."
    contexts = {
        built-artifact = "target:built-artifact"
    }
    dockerfile = "docker/tab_completions.Dockerfile"
}

target "vhs-base" {
    context = "."
    dockerfile = "docker/vhs_base.Dockerfile"
    contexts = {
        flyline-extracted-library = "target:built-artifact"
    }
    # Sets the hostname for the build sandbox; used by \h in the PS1 prompt during VHS recording.
    args = {
        BUILDKIT_SANDBOX_HOSTNAME = "my-hostname"
    }
}

target "demo-main-extracted-gif" {
    context = "."
    dockerfile = "docker/demo_main.Dockerfile"
    contexts = {
        vhs-base = "target:vhs-base"
    }
    target = "demo-main-extracted-gif"
    # Sets the hostname for the build sandbox; used by \h in the PS1 prompt during VHS recording.
    args = {
        BUILDKIT_SANDBOX_HOSTNAME = "my-hostname"
    }
}

target "demo-prompts-extracted-gif" {
    context = "."
    dockerfile = "docker/demo_prompts.Dockerfile"
    contexts = {
        vhs-base = "target:vhs-base"
    }
    target = "demo-prompts-extracted-gif"
    # Sets the hostname for the build sandbox; used by \h in the PS1 prompt during VHS recording.
    args = {
        BUILDKIT_SANDBOX_HOSTNAME = "my-hostname"
    }
}

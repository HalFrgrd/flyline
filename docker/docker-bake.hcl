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
    dockerfile = "docker/demo_base.Dockerfile"
    contexts = {
        flyline-extracted-library = "target:built-artifact"
    }
}

target "_demo-base" {
    context = "."
    contexts = {
        vhs-base = "target:vhs-base"
    }
    output = ["type=local,dest=assets/"]
    # Sets the hostname for the build sandbox; used by \h in the PS1 prompt during VHS recording.
    args = {
        BUILDKIT_SANDBOX_HOSTNAME = "my-hostname"
    }
}

target "demo-main-extracted-gif" {
    inherit = ["_demo-base"]
        contexts = {
        vhs-base = "target:vhs-base"
    }
    dockerfile = "docker/demo_main.Dockerfile"
    target = "demo-main-extracted-gif"
}

target "demo-prompts-extracted-gif" {
    inherit = ["_demo-base"]
        contexts = {
        vhs-base = "target:vhs-base"
    }
    dockerfile = "docker/demo_prompts.Dockerfile"
    target = "demo-prompts-extracted-gif"
}

target "demo-fuzzy-suggestions-extracted-gif" {
    inherit = ["_demo-base"]
    contexts = {
        vhs-base = "target:vhs-base"
    }
    dockerfile = "docker/demo_fuzzy_suggestions.Dockerfile"
    target = "demo-fuzzy-suggestions-extracted-gif"
}

group "demos" {
    targets = [
        "demo-main-extracted-gif",
        "demo-prompts-extracted-gif",
        "demo-fuzzy-suggestions-extracted-gif",
    ]
}

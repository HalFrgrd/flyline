variable "BASH_VERSION_MATRIX" {
    default = ["4.4-rc1", "4.4.18", "5.0", "5.1.16", "5.2", "5.3"]
}

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

target "specific-bash-version" {
    context = "."
    dockerfile = "docker/specific_bash_version.Dockerfile"
    name = "specific-bash-version-${replace(docker_bash_version, ".", "_")}"
    matrix = {
        docker_bash_version = BASH_VERSION_MATRIX
    }
    args = {
        DOCKER_BASH_VERSION = docker_bash_version
    }
    tags = ["bash-${docker_bash_version}"]
}

target "bash-integration-tests" {
    context = "."
    contexts = {
        built-artifact = "target:built-artifact",
        specific-bash-version = "target:specific-bash-version-${replace(docker_bash_version, ".", "_")}"
    }
    name = "bash-integration-test-${replace(docker_bash_version, ".", "_")}"
    matrix = {
        docker_bash_version = BASH_VERSION_MATRIX
    }
    dockerfile = "docker/bash_integration_test.Dockerfile"
    args = {
        DOCKER_BASH_VERSION = docker_bash_version
    }
}


target "tab-completion-tests" {
    context = "."
    contexts = {
        built-artifact = "target:built-artifact"
    }
    dockerfile = "docker/tab_completions.Dockerfile"
}

# Runs `flyline --help` inside an interactive bash session, strips ANSI codes,
# and outputs flyline_help.txt to the project root.
target "extract-help-text" {
    context = "."
    contexts = {
        built-artifact = "target:built-artifact"
    }
    output = ["type=local,dest=./"]
    dockerfile = "docker/flyline_help.Dockerfile"
    target = "flyline-help-output"
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
    output = ["type=local,dest=./"]
    # Sets the hostname for the build sandbox; used by \h in the PS1 prompt during VHS recording.
    args = {
        BUILDKIT_SANDBOX_HOSTNAME = "my-hostname"
    }
}


target "demo-overview-extracted-gif" {
    inherits = ["_demo-base"]
    dockerfile = "docker/demo_overview.Dockerfile"
}

target "demo-prompts-extracted-gif" {
    inherits = ["_demo-base"]
    dockerfile = "docker/demo_prompts.Dockerfile"
}

target "demo-fuzzy-suggestions-extracted-gif" {
    inherits = ["_demo-base"]
    dockerfile = "docker/demo_fuzzy_suggestions.Dockerfile"
}

target "demo-custom-animation-extracted-gif" {
    inherits = ["_demo-base"]
    dockerfile = "docker/demo_custom_animation.Dockerfile"
}

target "demo-agent-mode-extracted-gif" {
    inherits = ["_demo-base"]
    dockerfile = "docker/demo_agent_mode.Dockerfile"
}

target "demo-ls-colors-extracted-gif" {
    inherits = ["_demo-base"]
    dockerfile = "docker/demo_ls_colors.Dockerfile"
}

group "demos" {
    targets = [
        "demo-overview-extracted-gif",
        "demo-prompts-extracted-gif",
        "demo-fuzzy-suggestions-extracted-gif",
        "demo-custom-animation-extracted-gif",
        "demo-agent-mode-extracted-gif",
        "demo-ls-colors-extracted-gif",
    ]
}

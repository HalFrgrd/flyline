variable "BASH_VERSION_MATRIX" {
    default = ["4.4-rc1", "4.4.18", "5.0", "5.1.16", "5.2", "5.3"]
}

variable "PRE_BASH_4_4_VERSION_MATRIX" {
    default = ["3.2.57"]
}

variable "FLYLINE_RELEASE_VERSION" {
    default = null
}

target "builder" {
    context = "."
    dockerfile = "docker/builder.Dockerfile"
    target = "flyline-builder"
}

target "built-artifact" {
    context = "."
    dockerfile = "docker/builder.Dockerfile"
    target = "flyline-built-artifact"
}

# example command:
# docker buildx bake -f docker/docker-bake.hcl extract-release-artifact
target "extract-release-artifact" {
    context = "."
    output = ["type=local,dest=docker/build"]
    dockerfile = "docker/builder.Dockerfile"
    target = "flyline-built-artifact"
}

target "extract-integration-test-build-artifact" {
    context = "."
    output = ["type=local,dest=docker/build-integration-test"]
    dockerfile = "docker/builder.Dockerfile"
    target = "flyline-built-artifact"
    args = {
        CARGO_FEATURES = "integration-tests"
    }
}

target "extract-pre-bash-4-4-integration-test-build-artifact" {
    context = "."
    output = ["type=local,dest=docker/build-pre-bash-4-4-integration-test"]
    dockerfile = "docker/builder.Dockerfile"
    target = "flyline-built-artifact"
    args = {
        CARGO_FEATURES = "integration-tests,pre_bash_4_4"
    }
}

target "lib-tests" {
    context = "."
    dockerfile = "docker/builder.Dockerfile"
    target = "flyline-lib-tests"
    args = {
        CARGO_FEATURES = "integration-tests"
    }
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

target "specific-bash-version-pre-4-4" {
    context = "."
    dockerfile = "docker/specific_bash_version.Dockerfile"
    name = "specific-bash-version-${replace(docker_bash_version, ".", "_")}"
    matrix = {
        docker_bash_version = PRE_BASH_4_4_VERSION_MATRIX
    }
    args = {
        DOCKER_BASH_VERSION = docker_bash_version
    }
    tags = ["bash-${docker_bash_version}"]
}

target "bash-integration-tests" {
    context = "."
    contexts = {
        built-artifact = "target:extract-integration-test-build-artifact",
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

target "bash-integration-tests-pre-4-4" {
    context = "."
    contexts = {
        built-artifact = "target:extract-pre-bash-4-4-integration-test-build-artifact",
        specific-bash-version = "target:specific-bash-version-${replace(docker_bash_version, ".", "_")}"
    }
    name = "bash-integration-test-${replace(docker_bash_version, ".", "_")}"
    tags = ["bash-integration-test-pre-4-4-${docker_bash_version}"]
    matrix = {
        docker_bash_version = PRE_BASH_4_4_VERSION_MATRIX
    }
    dockerfile = "docker/bash_integration_test.Dockerfile"
    args = {
        DOCKER_BASH_VERSION = docker_bash_version
    }
}


target "tab-completion-tests" {
    context = "."
    contexts = {
        built-artifact = "target:extract-integration-test-build-artifact"
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


target "demo-base" {
    context = "."
    dockerfile = "docker/demo_base.Dockerfile"
    contexts = {
        flyline-extracted-library = "target:built-artifact"
    }
}

target "_demo-base" {
    context = "."
    contexts = {
        demo-base = "target:demo-base"
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

target "demo-fuzzy-history-extracted-gif" {
    inherits = ["_demo-base"]
    dockerfile = "docker/demo_fuzzy_history.Dockerfile"
}

group "demos" {
    targets = [
        "demo-overview-extracted-gif",
        "demo-prompts-extracted-gif",
        "demo-fuzzy-suggestions-extracted-gif",
        "demo-custom-animation-extracted-gif",
        "demo-agent-mode-extracted-gif",
        "demo-ls-colors-extracted-gif",
        "demo-fuzzy-history-extracted-gif"
    ]
}

target "install-test-alpine" {
    context = "."
    dockerfile = "docker/install_test_alpine.Dockerfile"
    args = {
        FLYLINE_RELEASE_VERSION = FLYLINE_RELEASE_VERSION
    }
}

target "install-test-ubuntu" {
    context = "."
    dockerfile = "docker/install_test_ubuntu.Dockerfile"
    args = {
        FLYLINE_RELEASE_VERSION = FLYLINE_RELEASE_VERSION
    }
}

target "install-test-pre-bash-3-2-57" {
    context = "."
    contexts = {
        specific-bash-version = "target:specific-bash-version-3_2_57"
    }
    dockerfile = "docker/install_test_pre_bash_3_2_57.Dockerfile"
    args = {
        FLYLINE_RELEASE_VERSION = FLYLINE_RELEASE_VERSION
    }
}


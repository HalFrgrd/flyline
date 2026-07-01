variable "BASH_VERSION_MATRIX" {
    default = ["4.4-rc1", "4.4.18", "5.0", "5.3"]
}

variable "PRE_BASH_4_4_VERSION_MATRIX" {
    default = ["3.2.57"]
}

variable "FLYLINE_INSTALL_VERSION" {
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
# docker buildx bake -f docker-bake.hcl extract-release-artifact
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
}

target "extract-pre-bash-4-4-integration-test-build-artifact" {
    context = "."
    output = ["type=local,dest=docker/build-pre-bash-4-4-integration-test"]
    dockerfile = "docker/builder.Dockerfile"
    target = "flyline-built-artifact"
    args = {
        CARGO_FEATURES = "pre_bash_4_4"
    }
}

target "lib-tests" {
    context = "."
    dockerfile = "docker/builder.Dockerfile"
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


target "demo-overview-extracted" {
    inherits = ["_demo-base"]
    dockerfile = "docker/demo_overview.Dockerfile"
}

target "demo-prompts-extracted" {
    inherits = ["_demo-base"]
    dockerfile = "docker/demo_prompts.Dockerfile"
}

target "demo-fuzzy-suggestions-extracted" {
    inherits = ["_demo-base"]
    dockerfile = "docker/demo_fuzzy_suggestions.Dockerfile"
}

target "demo-fuzzy-path-suggestions-extracted" {
    inherits = ["_demo-base"]
    dockerfile = "docker/demo_fuzzy_path_suggestions.Dockerfile"
}

target "demo-custom-animation-extracted" {
    inherits = ["_demo-base"]
    dockerfile = "docker/demo_custom_animation.Dockerfile"
}

target "demo-agent-mode-extracted" {
    inherits = ["_demo-base"]
    dockerfile = "docker/demo_agent_mode.Dockerfile"
}

target "demo-ls-colors-extracted" {
    inherits = ["_demo-base"]
    dockerfile = "docker/demo_ls_colors.Dockerfile"
}

target "demo-fuzzy-history-extracted" {
    inherits = ["_demo-base"]
    dockerfile = "docker/demo_fuzzy_history.Dockerfile"
}

target "demo-inline-history-extracted" {
    inherits = ["_demo-base"]
    dockerfile = "docker/demo_inline_history.Dockerfile"
}

target "demo-tab-completion-easing-extracted" {
    inherits = ["_demo-base"]
    dockerfile = "docker/demo_tab_completion_easing.Dockerfile"
}

target "demo-auto-tab-completion-extracted" {
    inherits = ["_demo-base"]
    dockerfile = "docker/demo_auto_tab_completion.Dockerfile"
}

target "demo-flycomp-extracted" {
    inherits = ["_demo-base"]
    dockerfile = "docker/demo_flycomp.Dockerfile"
}

group "demos" {
    targets = [
        "demo-overview-extracted",
        "demo-prompts-extracted",
        "demo-fuzzy-suggestions-extracted",
        "demo-fuzzy-path-suggestions-extracted",
        "demo-custom-animation-extracted",
        "demo-agent-mode-extracted",
        "demo-ls-colors-extracted",
        "demo-fuzzy-history-extracted",
        "demo-inline-history-extracted",
        "demo-tab-completion-easing-extracted",
        "demo-auto-tab-completion-extracted",
        "demo-flycomp-extracted"
    ]
}

target "install-test-alpine" {
    context = "."
    dockerfile = "docker/install_test_alpine.Dockerfile"
    args = {
        FLYLINE_INSTALL_VERSION = FLYLINE_INSTALL_VERSION
    }
}

target "install-test-ubuntu" {
    context = "."
    dockerfile = "docker/install_test_ubuntu.Dockerfile"
    args = {
        FLYLINE_INSTALL_VERSION = FLYLINE_INSTALL_VERSION
    }
}

target "install-test-bash-3-2-57" {
    context = "."
    contexts = {
        specific-bash-version = "target:specific-bash-version-3_2_57"
    }
    dockerfile = "docker/install_test_bash_3.2.57.Dockerfile"
    args = {
        FLYLINE_INSTALL_VERSION = FLYLINE_INSTALL_VERSION
    }
}


FROM ubuntu:20.04 AS help-runner

# ubuntu:20.04 ships with bash 5.0; no extra install needed.
# Prevent interactive prompts during any potential package installation
ENV DEBIAN_FRONTEND=noninteractive
# Disable color output from clap so the saved text is plain ASCII
ENV NO_COLOR=1

COPY --from=built-artifact /libflyline.so /libflyline.so

# Enable flyline on interactive bash startup
RUN touch /root/.bashrc && \
    printf 'enable -f /libflyline.so flyline\n' >> /root/.bashrc

# Run flyline --help in an interactive bash session and strip any residual
# ANSI escape sequences before saving the output.
RUN /bin/bash -i -c 'NO_COLOR=1 flyline --help' 2>/dev/null > /flyline_help.txt


# Run flyline create-prompt-widget animation --help and strip ANSI escape sequences.
RUN /bin/bash -i -c 'NO_COLOR=1 flyline create-prompt-widget animation --help' 2>/dev/null > /flyline_create_prompt_widget_animation_help.txt

# Run flyline create-prompt-widget mouse-mode --help and strip ANSI escape sequences.
RUN /bin/bash -i -c 'NO_COLOR=1 flyline create-prompt-widget mouse-mode --help' 2>/dev/null > /flyline_create_prompt_widget_mouse_mode_help.txt

# Run flyline create-prompt-widget custom --help and strip ANSI escape sequences.
RUN /bin/bash -i -c 'NO_COLOR=1 flyline create-prompt-widget custom --help' 2>/dev/null > /flyline_create_prompt_widget_custom_help.txt


FROM scratch AS flyline-help-output
COPY --from=help-runner /flyline_help.txt /flyline_help.txt
COPY --from=help-runner /flyline_create_prompt_widget_animation_help.txt /flyline_create_prompt_widget_animation_help.txt
COPY --from=help-runner /flyline_create_prompt_widget_mouse_mode_help.txt /flyline_create_prompt_widget_mouse_mode_help.txt
COPY --from=help-runner /flyline_create_prompt_widget_custom_help.txt /flyline_create_prompt_widget_custom_help.txt

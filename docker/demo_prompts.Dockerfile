FROM vhs-base AS demo-prompts-build

# Sets the hostname for the build sandbox; used by \h in the PS1 prompt during VHS recording.
ARG BUILDKIT_SANDBOX_HOSTNAME=my-hostname

# Override PS1 with a minimal prompt – the demo will set prompts interactively
RUN printf '%s\n' \
    'export PS1="bash$ "' \
    'export RPS1=""' \
    'export RPROMPT=""' \
    'export PS1_FILL=" "' \
    >> /home/john/.bashrc

COPY docker/demo_prompts.tape .

RUN vhs demo_prompts.tape

FROM scratch AS demo-prompts-extracted-gif
COPY --from=demo-prompts-build /app/demo_prompts.gif /demo_prompts.gif

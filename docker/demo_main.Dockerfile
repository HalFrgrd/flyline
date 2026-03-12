FROM vhs-base AS demo-main-build

# Sets the hostname for the build sandbox; used by \h in the PS1 prompt during VHS recording.
ARG BUILDKIT_SANDBOX_HOSTNAME=my-hostname

# The base image already provides `vhs` as the entrypoint; no override needed.
COPY docker/demo_main.tape .

RUN vhs demo_main.tape

FROM scratch AS demo-main-extracted-gif
COPY --from=demo-main-build /app/demo.gif /demo.gif

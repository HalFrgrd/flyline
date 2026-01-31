# Dockerfile for Charmbracelet VHS using official image as base
# Run with:
# docker build -f docker/Dockerfile.vhs --target vhs-extracted-gif -o . .

# Optional stage reference to the built library image available locally
FROM flyline-extracted-library AS flyline-extracted-library

# Use the published Charmbracelet VHS image by digest
FROM ghcr.io/charmbracelet/vhs@sha256:cbcdcd255e61dd9d278ad25183ae3662c849ab9b104434ac1ba330d27b2883cc AS vhs-base

WORKDIR /app

# Copy the Flyline shared library into the container
COPY --from=flyline-extracted-library /libflyline.so .
RUN touch /root/.bashrc && \
    printf '%s\n' \
    'alias ll="ls -alF"' \
    'export HISTTIMEFORMAT="%F %T  "' \
    'enable -f /app/libflyline.so flyline' \
    >> /root/.bashrc

# The base image already provides `vhs` as the entrypoint; no override needed.
COPY docker/vhs_demo.tape .

RUN vhs vhs_demo.tape

FROM scratch AS vhs-extracted-gif
COPY --from=vhs-base /app/demo.gif /demo.gif

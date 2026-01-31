ARG UBUNTU_VERSION

# Stage 1: Builder - Build the Flyline shared library
FROM flyline-extracted-library

# Stage 2: Ubuntu Testing - Test the built library on specified Ubuntu version
FROM ubuntu:${UBUNTU_VERSION} AS ubuntu_testing

# Prevent interactive prompts during package installation
ENV DEBIAN_FRONTEND=noninteractive

# Install basic dependencies and bash
RUN apt-get update && apt-get install -y \
    bash \
    binutils \
    && rm -rf /var/lib/apt/lists/*

# Set up working directory
WORKDIR /app

# Copy the built shared library from builder stage
COPY --from=flyline-extracted-library /libflyline.so .

# Write helpful comments to root's bashrc instead of copying a file
RUN touch /root/.bashrc && \
    printf '%s\n' \
    'alias ll="ls -alF"' \
    'export HISTTIMEFORMAT="%F %T  "' \
    'enable -f /app/libflyline.so flyline' \
    >> /root/.bashrc

# Default command to run the test
CMD ["/bin/bash", "-i", "-c", "flyline -s && flyline -v && echo 'SUCCESS: Test completed'"]

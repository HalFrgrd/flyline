ARG UBUNTU_VERSION

# Stage 1: Builder - Build the Flyline shared library
FROM flyline-extracted-library

# Stage 2: Ubuntu Testing - Test the built library on specified Ubuntu version
FROM ubuntu:${UBUNTU_VERSION} AS ubuntu_testing

# Copy the built shared library from builder stage
COPY --from=flyline-extracted-library /libflyline.so .

# Write helpful comments to root's bashrc instead of copying a file
RUN touch /root/.bashrc && \
    printf '%s\n' \
    'alias ll="ls -alF"' \
    'export HISTTIMEFORMAT="%F %T  "' \
    'enable -f /libflyline.so flyline' \
    >> /root/.bashrc

# Default command to run the test
RUN /bin/bash -i -c "flyline -s && flyline -v && echo 'SUCCESS: Test completed'"

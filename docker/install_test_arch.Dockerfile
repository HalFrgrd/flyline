FROM archlinux:latest

ARG FLYLINE_INSTALL_VERSION

RUN pacman -Syu --noconfirm && \
    pacman -S --noconfirm curl bash tar util-linux

RUN curl -sSfL https://github.com/HalFrgrd/flyline/releases/download/${FLYLINE_INSTALL_VERSION}/install.sh | FLYLINE_INSTALL_VERSION=${FLYLINE_INSTALL_VERSION} sh

# Run bash interactively to load flyline and test that it doesn't crash/fail on Arch Linux's strict dynamic linking environment
RUN script -q -c "/bin/bash -i -c 'flyline --version'" /dev/null

FROM termux/termux-docker:latest

ARG FLYLINE_INSTALL_VERSION

RUN bash -c "FLYLINE_INSTALL_VERSION=${FLYLINE_INSTALL_VERSION} source <(curl -sSfL https://github.com/HalFrgrd/flyline/releases/download/${FLYLINE_INSTALL_VERSION}/install.sh)"

RUN /data/data/com.termux/files/usr/bin/env LD_PRELOAD=/data/data/com.termux/files/usr/lib/libreadline.so /data/data/com.termux/files/usr/bin/bash -i -c "flyline --version"

FROM termux/termux-docker:latest

ARG FLYLINE_INSTALL_VERSION

RUN script -q -e -c "/data/data/com.termux/files/usr/bin/bash -i -c 'FLYLINE_INSTALL_VERSION=${FLYLINE_INSTALL_VERSION} source <(curl -sSfL https://github.com/HalFrgrd/flyline/releases/download/${FLYLINE_INSTALL_VERSION}/install.sh)'" /dev/null

RUN script -q -e -c "/data/data/com.termux/files/usr/bin/bash -i -c 'flyline --version'" /dev/null

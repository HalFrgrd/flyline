FROM ubuntu:22.04 

ARG FLYLINE_INSTALL_VERSION

RUN apt-get update && apt-get install -y curl util-linux && rm -rf /var/lib/apt/lists/*


RUN script -q -e -c "/bin/bash -i -c 'FLYLINE_INSTALL_VERSION=${FLYLINE_INSTALL_VERSION} source <(curl -sSfL https://github.com/HalFrgrd/flyline/releases/download/${FLYLINE_INSTALL_VERSION}/install.sh)'" /dev/null

RUN script -q -e -c "/bin/bash -i -c 'flyline --version'" /dev/null

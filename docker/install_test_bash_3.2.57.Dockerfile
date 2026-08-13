FROM specific-bash-version

ARG FLYLINE_INSTALL_VERSION

RUN apt-get update && apt-get install -y curl util-linux && rm -rf /var/lib/apt/lists/*

RUN bash -c "FLYLINE_INSTALL_VERSION=${FLYLINE_INSTALL_VERSION} source <(curl -sSfL https://github.com/HalFrgrd/flyline/releases/download/${FLYLINE_INSTALL_VERSION}/install.sh)"

RUN script -q -c "/bin/bash -i -c 'flyline --version'" /dev/null

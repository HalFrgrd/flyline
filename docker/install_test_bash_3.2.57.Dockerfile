FROM specific-bash-version

ARG FLYLINE_INSTALL_VERSION

RUN apt-get update && apt-get install -y curl && rm -rf /var/lib/apt/lists/*

RUN curl -sSfL https://github.com/HalFrgrd/flyline/releases/download/${FLYLINE_INSTALL_VERSION}/install.sh | FLYLINE_INSTALL_VERSION=${FLYLINE_INSTALL_VERSION} sh

RUN /bin/bash -i -c "flyline --version"

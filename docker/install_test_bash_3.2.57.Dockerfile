FROM specific-bash-version

ARG FLYLINE_RELEASE_VERSION

RUN apt-get update && apt-get install -y curl && rm -rf /var/lib/apt/lists/*

RUN curl -sSfL https://github.com/HalFrgrd/flyline/releases/download/${FLYLINE_RELEASE_VERSION}/install.sh | FLYLINE_RELEASE_VERSION=${FLYLINE_RELEASE_VERSION} sh

RUN /bin/bash -i -c "flyline --version"

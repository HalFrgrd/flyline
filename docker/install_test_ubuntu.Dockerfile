FROM ubuntu:24.04 

ARG FLYLINE_RELEASE_VERSION

RUN apt-get update && apt-get install -y curl && rm -rf /var/lib/apt/lists/*


RUN curl -sSfL https://raw.githubusercontent.com/HalFrgrd/flyline/master/install.sh | FLYLINE_RELEASE_VERSION=${FLYLINE_RELEASE_VERSION} sh

RUN /bin/bash -i -c "flyline --version"

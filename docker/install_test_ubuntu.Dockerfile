FROM ubuntu:latest

ARG FLYLINE_RELEASE_VERSION

RUN apt-get update && apt-get install -y curl && rm -rf /var/lib/apt/lists/*

RUN FLYLINE_RELEASE_VERSION=${FLYLINE_RELEASE_VERSION} curl -sSfL https://raw.githubusercontent.com/HalFrgrd/flyline/main/install.sh | sh

RUN bash -i -c "flyline --version"

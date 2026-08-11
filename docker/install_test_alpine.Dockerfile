FROM alpine:latest

ARG FLYLINE_INSTALL_VERSION

RUN apk add --no-cache gcc bash curl util-linux

RUN curl -sSfL https://github.com/HalFrgrd/flyline/releases/download/${FLYLINE_INSTALL_VERSION}/install.sh | FLYLINE_INSTALL_VERSION=${FLYLINE_INSTALL_VERSION} sh

RUN script -q -c "bash -i -c 'flyline --version'" /dev/null

FROM alpine:latest

ARG FLYLINE_INSTALL_VERSION

RUN apk add --no-cache gcc bash curl util-linux

RUN script -q -e -c "bash -i -c 'FLYLINE_INSTALL_VERSION=${FLYLINE_INSTALL_VERSION} source <(curl -sSfL https://github.com/HalFrgrd/flyline/releases/download/${FLYLINE_INSTALL_VERSION}/install.sh)'" /dev/null

RUN script -q -e -c "bash -i -c 'flyline --version'" /dev/null

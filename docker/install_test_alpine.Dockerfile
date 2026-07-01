FROM alpine:latest

ARG FLYLINE_INSTALL_VERSION

RUN apk add --no-cache gcc bash curl

RUN curl -sSfL https://github.com/HalFrgrd/flyline/releases/download/${FLYLINE_INSTALL_VERSION}/install.sh | FLYLINE_INSTALL_VERSION=${FLYLINE_INSTALL_VERSION} sh

RUN bash -i -c "flyline --version"

FROM alpine:latest

ARG FLYLINE_RELEASE_VERSION

RUN apk add --no-cache gcc bash curl

RUN curl -sSfL https://raw.githubusercontent.com/HalFrgrd/flyline/master/install.sh | FLYLINE_RELEASE_VERSION=${FLYLINE_RELEASE_VERSION} sh

RUN bash -i -c "flyline --version"

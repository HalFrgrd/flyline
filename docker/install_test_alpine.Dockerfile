FROM alpine:latest

ARG FLYLINE_RELEASE_VERSION

RUN apk add --no-cache gcc bash curl

RUN FLYLINE_RELEASE_VERSION=${FLYLINE_RELEASE_VERSION} curl -sSfL https://raw.githubusercontent.com/HalFrgrd/flyline/main/install.sh | sh

RUN bash -i -c "flyline --version"

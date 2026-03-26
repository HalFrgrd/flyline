FROM alpine:latest

ARG FLYLINE_RELEASE_VERSION

RUN apk add --no-cache gcc bash curl

ENV FLYLINE_RELEASE_VERSION=${FLYLINE_RELEASE_VERSION}

RUN curl -sSfL https://raw.githubusercontent.com/HalFrgrd/flyline/master/install.sh | sh

RUN bash -i -c "flyline --version"

FROM ubuntu:24.04

ARG DOCKER_BASH_VERSION

RUN apt update && apt install -y \
  build-essential \
  ca-certificates \
  curl \
  libreadline-dev \
  libncurses-dev \
  && rm -rf /var/lib/apt/lists/*


WORKDIR /tmp/bash-build
RUN curl -LO https://ftp.gnu.org/gnu/bash/bash-${DOCKER_BASH_VERSION}.tar.gz \
 && tar xzf bash-${DOCKER_BASH_VERSION}.tar.gz \
 && cd bash-${DOCKER_BASH_VERSION} \
 && ./configure --prefix=/opt/bash-${DOCKER_BASH_VERSION} --with-readline \
 && make -j"$(nproc)" \
 && make install \
 && cd .. \
 && rm -rf bash-${DOCKER_BASH_VERSION}* 

RUN rm /bin/bash && ln -s /opt/bash-${DOCKER_BASH_VERSION}/bin/bash /bin/bash

FROM ubuntu:24.04

ARG BASH_VERSION

RUN apt update && apt install -y \
  build-essential \
  ca-certificates \
  curl \
  libreadline-dev \
  libncurses-dev \
  && rm -rf /var/lib/apt/lists/*


WORKDIR /tmp/bash-build
RUN curl -LO https://ftp.gnu.org/gnu/bash/bash-${BASH_VERSION}.tar.gz \
 && tar xzf bash-${BASH_VERSION}.tar.gz \
 && cd bash-${BASH_VERSION} \
 && ./configure --prefix=/opt/bash-${BASH_VERSION} --with-readline \
 && make -j"$(nproc)" \
 && make install \
 && cd .. \
 && rm -rf bash-${BASH_VERSION}* 

RUN rm /bin/bash && ln -s /opt/bash-${BASH_VERSION}/bin/bash /bin/bash

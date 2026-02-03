FROM ubuntu:24.04

RUN apt update && apt install -y \
  build-essential \
  curl \
  libreadline-dev \
  libncurses-dev

ARG BASH_VERSION

WORKDIR /tmp/bash-build
RUN curl -LO https://ftp.gnu.org/gnu/bash/bash-${BASH_VERSION}.tar.gz \
 && tar xzf bash-${BASH_VERSION}.tar.gz \
 && cd bash-${BASH_VERSION} \
 && ./configure --prefix=/opt/bash-${BASH_VERSION} --with-readline \
 && make -j \
 && make install

# Write helpful comments to root's bashrc instead of copying a file
RUN touch /root/.bashrc && \
printf '%s\n' \
'alias ll="ls -alF"' \
'export HISTTIMEFORMAT="%F %T  "' \
'enable -f /libflyline.so flyline' \
>> /root/.bashrc

RUN /opt/bash-${BASH_VERSION}/bin/bash --version

COPY libflyline.so /

RUN /opt/bash-${BASH_VERSION}/bin/bash -i -c "flyline --version && echo 'SUCCESS: Test completed'"
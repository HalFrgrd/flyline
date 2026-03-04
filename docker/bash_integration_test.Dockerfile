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

# Write helpful comments to root's bashrc instead of copying a file
RUN touch /root/.bashrc && \
printf '%s\n' \
'alias ll="ls -alF"' \
'export HISTTIMEFORMAT="%F %T  "' \
'enable -f /libflyline.so flyline' \
>> /root/.bashrc

RUN /opt/bash-${BASH_VERSION}/bin/bash --version

COPY --from=built-artifact /libflyline.so /

RUN /opt/bash-${BASH_VERSION}/bin/bash -i -c "flyline --version && echo 'SUCCESS: Test completed'"

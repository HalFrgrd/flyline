FROM debian:stable-slim

RUN apt-get update && \
    apt-get install -y curl ca-certificates build-essential

ARG BASH_VERSION

RUN curl -LO https://ftp.gnu.org/gnu/bash/bash-${BASH_VERSION}.tar.gz && \
    tar xzf bash-${BASH_VERSION}.tar.gz && \
    cd bash-${BASH_VERSION} && \
    ./configure CFLAGS="-Wno-error=implicit-function-declaration" && \
    make -j CFLAGS="-Wno-error=implicit-function-declaration" && \
    make install

# Write helpful comments to root's bashrc instead of copying a file
RUN touch /root/.bashrc && \
printf '%s\n' \
'alias ll="ls -alF"' \
'export HISTTIMEFORMAT="%F %T  "' \
'enable -f /libflyline.so flyline' \
>> /root/.bashrc

RUN /bin/bash --version

COPY libflyline.so /

RUN /bin/bash -i -c "flyline -s && flyline -v && echo 'SUCCESS: Test completed'"

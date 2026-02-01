FROM debian:stable-slim

RUN apt-get update && \
    apt-get install -y curl ca-certificates build-essential patch

ARG BASH_VERSION

RUN set -eux; \
    base_version="${BASH_VERSION%.*}"; \
    if curl -fLO "https://ftp.gnu.org/gnu/bash/bash-${BASH_VERSION}.tar.gz"; then \
        tar xzf "bash-${BASH_VERSION}.tar.gz"; \
        src_dir="bash-${BASH_VERSION}"; \
    else \
        curl -fLO "https://ftp.gnu.org/gnu/bash/bash-${base_version}.tar.gz"; \
        tar xzf "bash-${base_version}.tar.gz"; \
        src_dir="bash-${base_version}"; \
        if [ "${BASH_VERSION}" != "${base_version}" ]; then \
            patch_prefix="bash${base_version//./}"; \
            patch_level="${BASH_VERSION##*.}"; \
            cd "${src_dir}"; \
            for i in $(seq 1 "${patch_level}"); do \
                patch_file=$(printf '%s-%03d' "${patch_prefix}" "${i}"); \
                curl -fLO "https://ftp.gnu.org/gnu/bash/bash-${base_version}-patches/${patch_file}"; \
                patch -p0 < "${patch_file}"; \
            done; \
            cd ..; \
        fi; \
    fi; \
    cd "${src_dir}"; \
    ./configure CFLAGS="-Wno-error=implicit-function-declaration -Wno-error=implicit-int -Wno-implicit-int"; \
    make -j CFLAGS="-Wno-error=implicit-function-declaration -Wno-error=implicit-int -Wno-implicit-int"; \
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

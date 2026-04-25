FROM ubuntu:24.04

RUN apt update && apt install -y \
    bash-completion \
    && rm -rf /var/lib/apt/lists/*

COPY --from=built-artifact /libflyline.so /
COPY tests/completion_util.sh /opt/flyline/completion_util.sh

RUN set -eux; \
    mkdir -p  /tmp/example_fs/foo; \
    touch     /tmp/example_fs/foo/baz; \
    mkdir -p  /tmp/example_fs/foo/glob_stuff1; \
    touch     /tmp/example_fs/foo/glob_stuff1/.dotfile; \
    touch     /tmp/example_fs/foo/glob_stuff1/a.txt; \
    mkdir -p "/tmp/example_fs/many spaces here"; \
    touch    "/tmp/example_fs/file with spaces.txt"; \
    mkdir -p  /tmp/example_fs/abc/foo; \
    touch     /tmp/example_fs/abc/foo/baz; \
    touch     /tmp/example_fs/bar.txt; \
    ln -s foo /tmp/example_fs/sym_link_to_foo; \
    mkdir -p  /root/foo; \
    touch    '/root/foo/$baz.txt'

RUN set -eux; \
    cat > /root/tab_completion_test_bashrc <<'EOF'
set -e

source /usr/share/bash-completion/bash_completion
source /etc/bash_completion


enable -f /libflyline.so flyline
source /opt/flyline/completion_util.sh

export FOOBARBAZ="test"

flyline --version
flyline --run-tab-completion-tests

exit 0
EOF

RUN bash --version

WORKDIR /tmp/example_fs


RUN set -eux; \
    out="$(mktemp)"; \
    script -q -c "bash --rcfile /root/tab_completion_test_bashrc -i" /dev/null | tee "$out"; \
    grep -q "FLYLINE_TEST_SUCCESS" "$out"

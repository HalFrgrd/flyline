FROM ubuntu:24.04

RUN apt update && apt install -y \
    bash-completion \
    && rm -rf /var/lib/apt/lists/*

COPY --from=built-artifact /libflyline.so /
COPY tests/completion_util.sh /opt/flyline/completion_util.sh

RUN set -eux; \
    mkdir -p \
    /tmp/example_fs/foo \
    "/tmp/example_fs/many spaces here" \
    /tmp/example_fs/abc/foo; \
    printf '%s' content > /tmp/example_fs/bar.txt; \
    printf '%s' content > "/tmp/example_fs/file with spaces.txt"; \
    printf '%s' content > /tmp/example_fs/foo/baz; \
    printf '%s' content > /tmp/example_fs/abc/foo/baz; \
    ln -s foo /tmp/example_fs/sym_link_to_foo

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

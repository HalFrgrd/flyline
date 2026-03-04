FROM ubuntu:24.04

COPY --from=built-artifact /libflyline.so /
COPY tests/completion_util.sh /opt/flyline/completion_util.sh

RUN set -eux; \
	mkdir -p \
		/tmp/example_fs/foo \
		"/tmp/example_fs/many spaces here"; \
	printf '%s' content > /tmp/example_fs/bar.txt; \
	printf '%s' content > "/tmp/example_fs/file with spaces.txt"

RUN set -eux; \
	cat > /root/tab_completion_test_bashrc <<'EOF'
set -e

if ! shopt -oq posix; then
	if [ -f /usr/share/bash-completion/bash_completion ]; then
		. /usr/share/bash-completion/bash_completion
	elif [ -f /etc/bash_completion ]; then
		. /etc/bash_completion
	fi
fi

enable -f /libflyline.so flyline
source /opt/flyline/completion_util.sh

cd /tmp/example_fs

flyline --version
flyline --run-tab-completion-tests

exit 0
EOF

RUN /bin/bash --version

RUN set -eux; \
	out="$(mktemp)"; \
	script -q -c "/bin/bash --rcfile /root/tab_completion_test_bashrc -i" /dev/null | tee "$out"; \
	grep -q "FLYLINE_TEST_SUCCESS" "$out"

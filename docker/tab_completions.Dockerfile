FROM ubuntu:24.04


RUN apt update && apt install -y \
	# build-essential \
	# ca-certificates \
	# curl \
	# libreadline-dev \
	# libncurses-dev \
	bash-completion \
	# util-linux \
	&& rm -rf /var/lib/apt/lists/*

# ARG BASH_VERSION=5.3

# WORKDIR /tmp/bash-build
# RUN set -eux; \
# 	echo "BASH_VERSION=${BASH_VERSION}"; \
# 	curl -fsSLO "https://ftp.gnu.org/gnu/bash/bash-${BASH_VERSION}.tar.gz"; \
# 	tar xzf "bash-${BASH_VERSION}.tar.gz"; \
# 	cd "bash-${BASH_VERSION}"; \
# 	./configure --prefix="/opt/bash-${BASH_VERSION}" --with-readline; \
# 	make -j"$(nproc)"; \
# 	make install; \
# 	cd ..; \
# 	rm -rf "bash-${BASH_VERSION}"*

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


source /usr/share/bash-completion/bash_completion
# if ! shopt -oq posix; then
#     echo "Sourcing bash_completion for tab completion support"
# 	if [ -f /usr/share/bash-completion/bash_completion ]; then
#         echo "Sourcing bash_completion from /usr/share/bash-completion/bash_completion"

# 	elif [ -f /etc/bash_completion ]; then
#         echo "Sourcing bash_completion from /etc/bash_completion"
# 		. /etc/bash_completion
# 	fi
# else 
#     echo "POSIX mode detected, skipping bash_completion sourcing"
# fi


enable -f /libflyline.so flyline
source /opt/flyline/completion_util.sh

cd /tmp/example_fs

flyline --version
flyline --run-tab-completion-tests

exit 0
EOF

RUN bash --version


RUN set -eux; \
	out="$(mktemp)"; \
	script -q -c "/bin/bash --rcfile /root/tab_completion_test_bashrc -i" /dev/null | tee "$out"; \
	grep -q "FLYLINE_TEST_SUCCESS" "$out"

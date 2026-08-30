FROM demo-base AS demo-builder

# Override PS1 with a minimal prompt and ensure extglob is enabled
RUN printf '%s\n' \
    'shopt -s extglob' \
    'RPS1=""' \
    'RPROMPT=""' \
    'PS1_FILL=" "' \
    >> /home/john/.bashrc

RUN set -eux; \
    mkdir -p /home/john/project/src /home/john/project/docs /home/john/project/tests; \
    touch /home/john/project/Cargo.toml; \
    touch /home/john/project/Cargo.lock; \
    touch /home/john/project/README.md; \
    touch /home/john/project/build.rs; \
    touch /home/john/project/src/main.rs; \
    touch /home/john/project/src/lib.rs; \
    touch /home/john/project/docs/guide.pdf; \
    touch /home/john/project/docs/api.html; \
    touch /home/john/project/tests/test_basic.rs; \
    touch /home/john/project/tests/test_extglob.rs;

COPY tapes/demo_glob_expansion.tape .

RUN faketime @1771881894 /home/john/bin/evp demo_glob_expansion.tape

FROM scratch
COPY --from=demo-builder /app/*.gif /app/*.svg /

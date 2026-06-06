FROM demo-base AS demo-builder

# Override PS1 with a minimal prompt for the demo
RUN printf '%s\n' \
    # 'PS1="bash$ "' \
    'RPS1=""' \
    'export RPROMPT=""' \
    'PS1_FILL=" "' \
    >> /home/john/.bashrc

RUN set -eux; \
    mkdir -p /home/john/fruits; \
    touch /home/john/fruits/apple.txt; \
    touch /home/john/fruits/banana.txt; \
    touch /home/john/fruits/orange.txt;

COPY tapes/demo_fuzzy_path_suggestions.tape .

RUN faketime @1771881894 /home/john/bin/evp demo_fuzzy_path_suggestions.tape

FROM scratch
COPY --from=demo-builder /app/*.gif /app/*.svg /

FROM demo-base AS gif-builder

# Override PS1 with a minimal prompt for the demo
RUN printf '%s\n' \
    # 'PS1="bash$ "' \
    'RPS1=""' \
    'export RPROMPT=""' \
    'PS1_FILL=" "' \
    >> /home/john/.bashrc

RUN set -eux; \
    touch /home/john/apple.txt; \
    touch /home/john/banana.txt; \
    touch /home/john/orange.txt; \
    rm -r /home/john/bin;

COPY tapes/demo_fuzzy_path_suggestions.tape .

RUN faketime @1771881894 vhs demo_fuzzy_path_suggestions.tape

FROM scratch
COPY --from=gif-builder /app/*.gif /

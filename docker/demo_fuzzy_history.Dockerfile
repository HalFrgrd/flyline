FROM demo-base AS demo-builder

# Override PS1 with a minimal prompt for the demo
RUN printf '%s\n' \
    # 'PS1="bash$ "' \
    'RPS1=""' \
    'export RPROMPT=""' \
    'PS1_FILL=" "' \
    >> /home/john/.bashrc


COPY tapes/demo_fuzzy_history.tape .

RUN faketime @1771881894 /home/john/bin/evp demo_fuzzy_history.tape

FROM scratch
COPY --from=demo-builder /app/*.gif /app/*.svg /

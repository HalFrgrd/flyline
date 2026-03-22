FROM demo-base AS gif-builder

# Override PS1 with a minimal prompt for the demo
RUN printf '%s\n' \
    # 'export PS1="bash$ "' \
    'export RPS1=""' \
    'export RPROMPT=""' \
    'export PS1_FILL=" "' \
    >> /home/john/.bashrc


COPY tapes/demo_fuzzy_history.tape .

RUN faketime @1771881894 vhs demo_fuzzy_history.tape

FROM scratch
COPY --from=gif-builder /app/*.gif /

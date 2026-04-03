FROM demo-base AS gif-builder

# Override PS1 with a minimal prompt for the demo
RUN printf '%s\n' \
    # 'PS1="bash$ "' \
    'RPS1=""' \
    'export RPROMPT=""' \
    'PS1_FILL=" "' \
    >> /home/john/.bashrc


COPY tapes/demo_fuzzy_suggestions.tape .

RUN faketime @1771881894 vhs demo_fuzzy_suggestions.tape

FROM scratch
COPY --from=gif-builder /app/*.gif /

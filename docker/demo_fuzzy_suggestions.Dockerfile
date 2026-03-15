FROM vhs-base AS gif-builder

# Override PS1 with a minimal prompt for the demo
RUN printf '%s\n' \
    # 'export PS1="bash$ "' \
    'export RPS1=""' \
    'export RPROMPT=""' \
    'export PS1_FILL=" "' \
    >> /home/john/.bashrc


COPY assets/tapes/demo_fuzzy_suggestions.tape .

RUN faketime @1771881894 vhs demo_fuzzy_suggestions.tape

FROM scratch
COPY --from=gif-builder /app/*.gif /

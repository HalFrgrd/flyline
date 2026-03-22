FROM demo-base AS gif-builder

# Override PS1 with a minimal prompt – the demo will set prompts interactively
RUN printf '%s\n' \
    'export PS1="bash$ "' \
    'export RPS1=""' \
    'export RPROMPT=""' \
    'export PS1_FILL=" "' \
    >> /home/john/.bashrc


COPY tapes/demo_prompts*.tape .

RUN faketime @1771881894 vhs demo_prompts_ps1.tape
RUN faketime @1771881894 vhs demo_prompts_rps1.tape
RUN faketime @1771881894 vhs demo_prompts_ps1_fill.tape

FROM scratch
COPY --from=gif-builder /app/*.gif /

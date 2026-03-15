FROM vhs-base AS demo-prompts-build

# Override PS1 with a minimal prompt – the demo will set prompts interactively
RUN printf '%s\n' \
    'export PS1="bash$ "' \
    'export RPS1=""' \
    'export RPROMPT=""' \
    'export PS1_FILL=" "' \
    >> /home/john/.bashrc


COPY assets/tapes/demo_prompts*.tape .

RUN faketime @1771881894 vhs demo_prompts_ps1.tape
RUN faketime @1771881894 vhs demo_prompts_rps1.tape
RUN faketime @1771881894 vhs demo_prompts_ps1_fill.tape

FROM scratch AS demo-prompts-extracted-gif
COPY --from=demo-prompts-build /app/*.gif /

FROM demo-base AS demo-builder

# Override PS1 with a minimal prompt – the demo will set prompts interactively
RUN printf '%s\n' \
    'PS1="bash$ "' \
    'RPS1=""' \
    'export RPROMPT=""' \
    'PS1_FILL=" "' \
    >> /home/john/.bashrc


COPY tapes/demo_prompts_ps1.tape .
RUN faketime @1771881894 /home/john/bin/evp demo_prompts_ps1.tape
COPY tapes/demo_prompts_rps1.tape .
RUN faketime @1771881894 /home/john/bin/evp demo_prompts_rps1.tape
COPY tapes/demo_prompts_ps1_fill.tape .
RUN faketime @1771881894 /home/john/bin/evp demo_prompts_ps1_fill.tape
COPY tapes/demo_prompts_final.tape .
RUN faketime @1771881894 /home/john/bin/evp demo_prompts_final.tape

FROM scratch
COPY --from=demo-builder /app/*.gif /app/*.svg /

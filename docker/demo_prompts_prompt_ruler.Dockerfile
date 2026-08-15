FROM demo-base AS demo-builder

RUN printf '%s\n' \
    'PS1="\e[01;32m\u@\h\e[00m:\e[01;34m\w\e[00m\n$ "' \
    'RPS1=""' \
    'RPROMPT=""' \
    >> /home/john/.bashrc

COPY tapes/demo_prompts_prompt_ruler.tape .
RUN faketime @1771881894 /home/john/bin/evp demo_prompts_prompt_ruler.tape

FROM scratch
COPY --from=demo-builder /app/*.gif /app/*.svg /

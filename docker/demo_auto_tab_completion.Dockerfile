FROM demo-base AS demo-builder

COPY tapes/demo_auto_tab_completion.tape .

RUN faketime @1771881894 /home/john/bin/evp demo_auto_tab_completion.tape

FROM scratch
COPY --from=demo-builder /app/*.gif /app/*.svg /

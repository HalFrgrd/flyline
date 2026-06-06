FROM demo-base AS demo-builder

COPY tapes/demo_agent_mode.tape .

RUN faketime @1771881894 /home/john/bin/evp demo_agent_mode.tape

FROM scratch
COPY --from=demo-builder /app/*.gif /app/*.svg /
COPY --from=demo-builder /home/john/*log  /

FROM demo-base AS gif-builder

COPY tapes/demo_agent_mode.tape .


RUN faketime @1771881894 vhs demo_agent_mode.tape

FROM scratch
COPY --from=gif-builder /app/*.gif /
COPY --from=gif-builder /home/john/*log  /

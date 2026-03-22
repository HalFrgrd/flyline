FROM demo-base AS gif-builder

COPY tapes/demo_overview.tape .

RUN faketime @1771881894 vhs demo_overview.tape

FROM scratch
COPY --from=gif-builder /app/*.gif /

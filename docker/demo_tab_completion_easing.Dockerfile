FROM demo-base AS gif-builder

COPY tapes/demo_tab_completion_easing.tape .

RUN faketime @1771881894 vhs demo_tab_completion_easing.tape

FROM scratch
COPY --from=gif-builder /app/*.gif /

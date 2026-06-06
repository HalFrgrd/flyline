FROM demo-base AS demo-builder

COPY tapes/demo_custom_animation.tape .

RUN faketime @1771881894 /home/john/bin/evp demo_custom_animation.tape

FROM scratch
COPY --from=demo-builder /app/*.gif /app/*.svg /

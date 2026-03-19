FROM vhs-base AS gif-builder

COPY tapes/demo_custom_animation.tape .

RUN faketime @1771881894 vhs demo_custom_animation.tape

FROM scratch
COPY --from=gif-builder /app/*.gif /

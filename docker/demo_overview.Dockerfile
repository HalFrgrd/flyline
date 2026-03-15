FROM vhs-base AS demo-main-build

# The base image already provides `vhs` as the entrypoint; no override needed.
COPY assets/tapes/demo_overview.tape .

RUN faketime @1771881894 vhs demo_overview.tape

FROM scratch AS demo-main-extracted-gif
COPY --from=demo-main-build /app/*.gif /

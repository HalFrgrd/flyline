FROM vhs-base AS demo-overview-build

# The base image already provides `vhs` as the entrypoint; no override needed.
COPY assets/tapes/demo_overview.tape .

RUN faketime @1771881894 vhs demo_overview.tape

FROM scratch AS demo-overview-extracted-gif
COPY --from=demo-overview-build /app/*.gif /

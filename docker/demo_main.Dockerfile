FROM vhs-base AS demo-main-build

# The base image already provides `vhs` as the entrypoint; no override needed.
COPY docker/demo_main.tape .

RUN vhs demo_main.tape

FROM scratch AS demo-main-extracted-gif
COPY --from=demo-main-build /app/demo.gif /demo.gif

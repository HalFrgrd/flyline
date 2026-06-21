FROM demo-base AS demo-builder

USER root
RUN apt-get update && apt-get install -y --no-install-recommends \
    binutils \
    && rm -rf /var/lib/apt/lists/*
USER john

# Override PS1 with a minimal prompt for the demo
RUN printf '%s\n' \
    'RPS1=""' \
    'export RPROMPT=""' \
    'PS1_FILL=" "' \
    >> /home/john/.bashrc

COPY tapes/demo_flycomp.tape .

RUN faketime @1771881894 /home/john/bin/evp demo_flycomp.tape

FROM scratch
COPY --from=demo-builder /app/*.gif /app/*.svg /

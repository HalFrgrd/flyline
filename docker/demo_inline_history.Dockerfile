FROM demo-base AS demo-builder

# Start with an empty history so the demo is deterministic.
RUN : > /home/john/.bash_history && \
    printf '%s\n' \
    'flyline editor --show-inline-history true' \
    >> /home/john/.bashrc

COPY tapes/demo_inline_history.tape .

RUN faketime @1771881894 /home/john/bin/evp demo_inline_history.tape

FROM scratch
COPY --from=demo-builder /app/*.gif  /app/*.svg /
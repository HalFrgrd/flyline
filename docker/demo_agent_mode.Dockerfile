FROM vhs-base AS gif-builder

# Install the mock claude executable: always sleeps 3 s then emits a fixed JSON array
RUN mkdir -p /home/john/bin
COPY docker/claude /home/john/bin/claude
USER root
RUN chmod +x /home/john/bin/claude
USER john
RUN echo 'export PATH="/home/john/bin:$PATH"' >> /home/john/.bashrc

COPY tapes/demo_agent_mode.tape .

RUN faketime @1771881894 vhs demo_agent_mode.tape

FROM scratch
COPY --from=gif-builder /app/*.gif /
COPY --from=gif-builder /home/john/*log  /

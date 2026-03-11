FROM vhs-base AS demo-prompts-build

# Override .bashrc with a minimal prompt – the demo will set prompts interactively
RUN printf '%s\n' \
    'export PS1="bash$ "' \
    'enable -f /app/libflyline.so flyline' \
    'flyline --disable-auto-closing-char' \
    > /root/.bashrc

COPY docker/demo_prompts.tape .

RUN vhs demo_prompts.tape

FROM scratch AS demo-prompts-extracted-gif
COPY --from=demo-prompts-build /app/demo_prompts.gif /demo_prompts.gif

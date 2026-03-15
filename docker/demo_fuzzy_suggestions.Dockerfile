FROM vhs-base AS demo-fuzzy-suggestions-build

# Override PS1 with a minimal prompt for the demo
RUN printf '%s\n' \
    'export PS1="bash$ "' \
    'export RPS1=""' \
    'export RPROMPT=""' \
    'export PS1_FILL=" "' \
    >> /home/john/.bashrc

COPY docker/demo_base.tape .
COPY docker/demo_fuzzy_suggestions.tape .

RUN faketime @1771881894 vhs demo_fuzzy_suggestions.tape

FROM scratch AS demo-fuzzy-suggestions-extracted-gif
COPY --from=demo-fuzzy-suggestions-build /app/demo_fuzzy_suggestions.gif /demo_fuzzy_suggestions.gif

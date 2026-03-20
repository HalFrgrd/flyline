FROM vhs-base AS gif-builder

# Create a demo filesystem with directories, regular files, a valid symlink to a
# directory, and a broken symlink (pointing to a non-existent target).
RUN mkdir -p /home/john/demo_ls/archive /home/john/demo_ls/projects && \
    touch /home/john/demo_ls/config.json /home/john/demo_ls/notes.txt && \
    ln -s /home/john/demo_ls/archive /home/john/demo_ls/workspace && \
    ln -s /nonexistent_target /home/john/demo_ls/old_data

# Configure LS_COLORS, add file completion for 'foo', and cd into the demo dir
RUN printf '%s\n' \
    'export LS_COLORS="di=01;34:ln=01;36:or=01;31:fi=00"' \
    'complete -f foo' \
    'cd ~/demo_ls' \
    >> /home/john/.bashrc

COPY tapes/demo_ls_colors.tape .

RUN faketime @1771881894 vhs demo_ls_colors.tape

FROM scratch
COPY --from=gif-builder /app/*.gif /

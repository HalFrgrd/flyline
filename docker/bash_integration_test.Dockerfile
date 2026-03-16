FROM specific-bash-version

# Write helpful comments to root's bashrc instead of copying a file
RUN touch /root/.bashrc && \
printf '%s\n' \
'alias ll="ls -alF"' \
'export HISTTIMEFORMAT="%F %T  "' \
'enable -f /libflyline.so flyline' \
>> /root/.bashrc

RUN /bin/bash --version

COPY --from=built-artifact /libflyline.so /

RUN /bin/bash -i -c "flyline --version && echo 'SUCCESS: Test completed'"

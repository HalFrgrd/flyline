ARG UBUNTU_VERSION

FROM ubuntu:${UBUNTU_VERSION}

# Write helpful comments to root's bashrc instead of copying a file
RUN touch /root/.bashrc && \
printf '%s\n' \
'alias ll="ls -alF"' \
'export HISTTIMEFORMAT="%F %T  "' \
'enable -f /libflyline.so flyline' \
>> /root/.bashrc

COPY libflyline.so /

# Default command to run the test
RUN /bin/bash -i -c "flyline -s && flyline -v && echo 'SUCCESS: Test completed'"

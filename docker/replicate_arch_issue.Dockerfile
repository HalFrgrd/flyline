FROM archlinux:latest

RUN pacman -Syu --noconfirm curl bash

RUN curl -sSfL https://github.com/HalFrgrd/flyline/releases/download/v1.2.5/install.sh | FLYLINE_INSTALL_VERSION=v1.2.5 sh

CMD ["bash", "-c", "enable -f /root/.local/lib/libflyline.so flyline"]

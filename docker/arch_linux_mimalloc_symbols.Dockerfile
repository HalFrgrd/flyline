# Issue #802 (Arch Linux AUR / `undefined symbol: mi_free`):
# Fix: Adding `CFLAGS+=' -ffat-lto-objects'` in PKGBUILD (_srcenv per Arch RFC 69) ensures
# static C symbols in `libmimalloc-sys` are preserved during LTO, keeping `libflyline.so` self-contained.

FROM archlinux:latest

RUN pacman -Sy --noconfirm \
    base-devel \
    rust \
    bash \
    git \
    sudo \
    util-linux \
    && rm -rf /var/cache/pacman/pkg/*

# Create a non-root builder user required by makepkg
RUN useradd -m builder && \
    echo "builder ALL=(ALL) NOPASSWD: ALL" >> /etc/sudoers

WORKDIR /home/builder/build
COPY . src/
COPY docker/PKGBUILD PKGBUILD

RUN chown -R builder:builder /home/builder

USER builder
RUN makepkg --noconfirm -s

USER root
RUN pacman -U flyline-*.pkg.tar.zst --noconfirm

# Inspect dynamic symbol table of installed package for undefined mimalloc (mi_) symbols
RUN nm -D /usr/lib/bash/libflyline.so | grep " U mi_" || true

# Test loading the installed flyline package in interactive bash
RUN script -q -c "bash -i -c 'enable -f /usr/lib/bash/libflyline.so flyline && echo \"SUCCESS: flyline AUR package loaded successfully!\"'" /dev/null

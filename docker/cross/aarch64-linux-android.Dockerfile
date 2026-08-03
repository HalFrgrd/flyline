FROM ghcr.io/cross-rs/aarch64-linux-android:main

# Create libunwind.a symlinks where the NDK gcc linker expects them
RUN find /android-ndk -name "libgcc.a" -exec sh -c 'for f; do ln -sf "$f" "$(dirname "$f")/libunwind.a"; done' _ {} + || true
RUN find /android-ndk -name "libunwind.a" -exec sh -c 'for f; do cp -f "$f" /android-ndk/sysroot/usr/lib/aarch64-linux-android/ 2>/dev/null || true; done' _ {} +
RUN find /android-ndk -name "*clang" -exec sh -c 'for c; do case "$c" in *aarch64*) "$c" -shared -o /android-ndk/sysroot/usr/lib/aarch64-linux-android/libreadline.so -x c /dev/null 2>/dev/null && exit 0 ;; esac; done' _ {} +

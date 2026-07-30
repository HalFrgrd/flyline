FROM ghcr.io/cross-rs/i686-linux-android:main

# Create libunwind.a symlinks where the NDK gcc linker expects them
RUN find /android-ndk -name "libgcc.a" -exec sh -c 'for f; do ln -sf "$f" "$(dirname "$f")/libunwind.a"; done' _ {} + || true
RUN find /android-ndk -name "libunwind.a" -exec sh -c 'for f; do cp -f "$f" /android-ndk/sysroot/usr/lib/i686-linux-android/ 2>/dev/null || true; done' _ {} +

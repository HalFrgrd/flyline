FROM ghcr.io/cross-rs/aarch64-unknown-linux-gnu:main-centos

# Create a compatibility stdatomic.h for GCC 4.8.5
RUN echo '#ifndef _COMPAT_STDATOMIC_H' > /usr/local/include/stdatomic.h && \
    echo '#define _COMPAT_STDATOMIC_H' >> /usr/local/include/stdatomic.h && \
    echo '#include <stddef.h>' >> /usr/local/include/stdatomic.h && \
    echo '#include <stdint.h>' >> /usr/local/include/stdatomic.h && \
    echo 'typedef enum {' >> /usr/local/include/stdatomic.h && \
    echo '    memory_order_relaxed = __ATOMIC_RELAXED,' >> /usr/local/include/stdatomic.h && \
    echo '    memory_order_consume = __ATOMIC_CONSUME,' >> /usr/local/include/stdatomic.h && \
    echo '    memory_order_acquire = __ATOMIC_ACQUIRE,' >> /usr/local/include/stdatomic.h && \
    echo '    memory_order_release = __ATOMIC_RELEASE,' >> /usr/local/include/stdatomic.h && \
    echo '    memory_order_acq_rel = __ATOMIC_ACQ_REL,' >> /usr/local/include/stdatomic.h && \
    echo '    memory_order_seq_cst = __ATOMIC_SEQ_CST' >> /usr/local/include/stdatomic.h && \
    echo '} memory_order;' >> /usr/local/include/stdatomic.h && \
    echo '#define _Atomic(T) T' >> /usr/local/include/stdatomic.h && \
    echo '#define atomic_load_explicit(object, order) __atomic_load_n(object, order)' >> /usr/local/include/stdatomic.h && \
    echo '#define atomic_store_explicit(object, desired, order) __atomic_store_n(object, desired, order)' >> /usr/local/include/stdatomic.h && \
    echo '#define atomic_exchange_explicit(object, desired, order) __atomic_exchange_n(object, desired, order)' >> /usr/local/include/stdatomic.h && \
    echo '#define atomic_compare_exchange_strong_explicit(object, expected, desired, success, failure) __atomic_compare_exchange_n(object, expected, desired, 0, success, failure)' >> /usr/local/include/stdatomic.h && \
    echo '#define atomic_compare_exchange_weak_explicit(object, expected, desired, success, failure) __atomic_compare_exchange_n(object, expected, desired, 1, success, failure)' >> /usr/local/include/stdatomic.h && \
    echo '#define atomic_fetch_add_explicit(object, operand, order) __atomic_fetch_add(object, operand, order)' >> /usr/local/include/stdatomic.h && \
    echo '#define atomic_fetch_sub_explicit(object, operand, order) __atomic_fetch_sub(object, operand, order)' >> /usr/local/include/stdatomic.h && \
    echo '#define atomic_fetch_and_explicit(object, operand, order) __atomic_fetch_and(object, operand, order)' >> /usr/local/include/stdatomic.h && \
    echo '#define atomic_fetch_or_explicit(object, operand, order) __atomic_fetch_or(object, operand, order)' >> /usr/local/include/stdatomic.h && \
    echo '#endif' >> /usr/local/include/stdatomic.h && \
    cp /usr/local/include/stdatomic.h /usr/aarch64-linux-gnu/include/stdatomic.h

# Create a compiler wrapper to filter out the unsupported -Wno-error=date-time flag and inject C11 flags
RUN for bin in gcc g++ aarch64-linux-gnu-gcc aarch64-linux-gnu-g++ x86_64-redhat-linux-gcc x86_64-redhat-linux-g++; do \
      if [ -f /usr/bin/$bin ]; then \
        mv /usr/bin/$bin /usr/bin/$bin.real && \
        echo '#!/bin/bash' > /usr/bin/$bin && \
        echo 'args=()' >> /usr/bin/$bin && \
        echo 'for arg in "$@"; do' >> /usr/bin/$bin && \
        echo '  if [[ "$arg" != "-Wno-error=date-time" && "$arg" != "-Werror=date-time" ]]; then' >> /usr/bin/$bin && \
        echo '    args+=("$arg")' >> /usr/bin/$bin && \
        echo '  fi' >> /usr/bin/$bin && \
        echo 'done' >> /usr/bin/$bin && \
        echo 'real_bin=$(readlink -f "$0")' >> /usr/bin/$bin && \
        echo 'binary_name=$(basename "$real_bin")' >> /usr/bin/$bin && \
        echo 'if [[ "$binary_name" != *g++* && "$binary_name" != *c++* ]]; then' >> /usr/bin/$bin && \
        echo '  args+=("-std=gnu11" "-D__has_include(x)=0")' >> /usr/bin/$bin && \
        echo 'fi' >> /usr/bin/$bin && \
        echo 'exec "$real_bin.real" "${args[@]}"' >> /usr/bin/$bin && \
        chmod +x /usr/bin/$bin; \
      fi; \
    done

# Threads and Locking

This document briefly explains the threading model and lock lifecycle implemented in `flyline`.

## Concurrency and FFI Safety
`flyline` runs inside the active host Bash process. Multiple Rust threads (e.g., the background cache warming thread) can potentially access Bash internal APIs or heap structures simultaneously, which causes memory corruption and crashes.

To prevent this, `flyline` enforces a global reentrant lock (`BASH_LOCK`).

## Locking Model

1. **Interactive Session (`get_command`)**:
   - While the user is typing, the main thread **does not** hold the global lock continuously.
   - Background threads (like the cache warming thread `"flyline-warming"`) can run concurrently with input editing.
   - Both the main thread and background threads must acquire `BASH_LOCK` briefly around individual Bash FFI function calls (e.g., fetching variables, aliases, or running command evaluations) to safely serialize access.

2. **Command Execution**:
   - When the user presses Enter and Flyline returns control to Bash, the background cache warming thread is joined and completed.
   - Because no background Rust threads remain running or calling Bash FFI functions while Bash is executing command execution C code, the main thread **does not need to hold the lock** during command execution.

3. **Deadlock Prevention**:
   - **Reentrancy**: `BASH_LOCK` is a `parking_lot::ReentrantMutex<()>`, allowing the same thread to acquire it recursively.
   - **Tab Completion Forking**: The background warming thread is joined and completed *before* calling `fork()` to ensure the child process does not inherit a locked mutex.


  ### Vector A: GLIBC Thread-Local Storage (TLS) Lifecycle Abort

  Empirical Evidence (GDB PID 588153):

    Thread 588153 (Main Thread):
      #6 internal_calloc (bash)
      #7 __cxa_thread_atexit_impl (glibc)
      #8 parking_lot_core::parking_lot::ThreadData::get_or_init_slow
    
    Thread 588174 (flyline-warming):
      #1 bash_allocator_guard_enter (hardened_bash)
      #3 __call_tls_dtors (glibc)

  • Mechanism:
      1. Accessing a Rust thread_local! variable (such as parking_lot's internal THREAD_DATA) for the first
      time on a thread causes glibc to invoke __cxa_thread_atexit_impl to register its destructor.
      2. When any thread terminates, glibc calls __call_tls_dtors to clean up thread-local variables.
      3. In Linux glibc, both __cxa_thread_atexit_impl and __call_tls_dtors internally allocate memory via
      standard C calloc/free.
      4. Because bash overrides calloc/free, glibc's TLS registration and destruction code calls Bash's
      single-threaded allocator!
      5. If Thread A accesses thread_local! (__cxa_thread_atexit_impl) while Thread B terminates
      (__call_tls_dtors), Bash's concurrency guard aborts the process.


### Vector B: Implicit C Library Allocations (opendir / std::fs::read_dir)

  Empirical Evidence (GDB PID 590938):

    Thread 590960 (PathWarming Thread):
      #0 internal_malloc (bash)
      #1 __alloc_dir (glibc opendir)
      #3 std::sys::fs::read_dir (libflyline.so)
    
    Thread 590938 (Main Thread):
      #3 internal_calloc (bash)
      #4 __cxa_thread_atexit_impl (glibc)

  • Mechanism:
      1. Standard C/POSIX filesystem functions (like opendir(), readdir(), getpwuid(), gethostbyname())
      inside glibc allocate internal stream buffers via C malloc().
      2. Calling Rust's std::fs::read_dir() on a background thread (PathWarming) calls glibc opendir(), which
      invokes internal_malloc() in bash.
      3. If the Main Thread performs any allocation simultaneously, hardened_bash aborts.


  ## 3. Comprehensive Engineering Design Rules

  To ensure stability, fast response times, and zero allocator aborts in flyline:

    ┌────────────────────────────────────────────────────────────────────────┐
    │                        FLYLINE CONCURRENCY LAWS                        │
    │                                                                        │
    │ 1. ZERO DYNAMIC THREAD CREATION / DESTRUCTION DURING RUNTIME            │
    │    All threads are pre-spawned ONCE at process startup (App::new).     │
    │                                                                        │
    │ 2. PRE-INITIALIZE THREAD-LOCAL STORAGE (TLS)                           │
    │    Pre-lock parking_lot during thread startup under a barrier.         │
    │                                                                        │
    │ 3. FORK MUTEX RESET                                                    │
    │    Immediately in child (if pid == 0), reset all inherited mutexes     │
    │    to clean, unlocked instances.                                       │
    │                                                                        │
    │ 4. MUTEX GUARDED ALLOCATIONS                                           │
    │    Acquire BASH_LOCK for any FFI call or opendir/read_dir invocation. │
    │                                                                        │
    │ 5. COMMAND EXECUTION LOCKING                                           │
    │    Hold BASH_LOCK on main thread while Bash runs user commands so     │
    │    background tasks safely wait until control returns to flyline.       │
    └────────────────────────────────────────────────────────────────────────┘
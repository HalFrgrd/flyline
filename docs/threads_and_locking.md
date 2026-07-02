# Threads and Locking

This document briefly explains the threading model and lock lifecycle implemented in `flyline`.

## Concurrency and FFI Safety
`flyline` runs inside the active host Bash process. Multiple Rust threads (e.g., the background cache warming thread) can potentially access Bash internal APIs or heap structures simultaneously, which causes memory corruption and crashes.

To prevent this, `flyline` enforces a global reentrant lock (`BASH_LOCK`).

## Locking Model

1. **Interactive Session (`get_command`)**:
   - While the user is typing, the main thread **does not** hold the global lock continuously.
   - Background threads (like the cache warming thread `"flyline-warming"`) can run concurrently with input editing.
   - Both the main thread and background threads must acquire `BASH_LOCK` briefly around individual Bash FFI function calls (e.g., fetching variables, aliases, or running command evaluations).

2. **Command Execution**:
   - When the user presses Enter and Flyline returns control to Bash, the main thread locks `BASH_LOCK` for the entire duration of the command execution.
   - This blocks background threads from executing Bash FFI functions while Bash is actively running commands.
   - The lock guard is stored in a thread-local static `COMMAND_LOCK_GUARD` on the main thread.

3. **Deadlock Prevention**:
   - **Reentrancy**: `BASH_LOCK` is a `parking_lot::ReentrantMutex<()>`, allowing the same thread to acquire it recursively.
   - **Thread Joining**: The main thread drops the command lock guard *before* joining background threads.
   - **Tab Completion Forking**: The background warming thread is joined and completed *before* calling `fork()` to ensure the child process does not inherit a locked mutex.

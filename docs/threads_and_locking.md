# Threads, forking, and locking

I used to use a few threads in flyline. But that caused problems.
- Starting and stopping threads calls glibc which calls bash's non threadsafe allocator. So we can't start and stop threads freely.
- libc stuff like opendir will allocate a struct using bash's non thread safe allocator. So it is hard to interact with the file system in a threadsafe way.

Even with using malloc as the global allocator, glibc would still call bash's non threadsafe allocator.

I can detect this with hardened_bash https://github.com/HalFrgrd/mirror-bash/tree/master.

My solution is to fork the process (like Im already doing for autosuggestions) when we really need parallelism.

Hopefully will fix issues: #891 and #915


We can have some threads. But they need to be very simple.
They can't be created when other threads might use bash's allocator.
They can't call into glibc concurrently.
So it's best not to use them.

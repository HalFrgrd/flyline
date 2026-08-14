use nix::poll::{PollFd, PollFlags, PollTimeout, poll};
use nix::unistd::{ForkResult, Pid, fork, pipe};
use serde::{Serialize, de::DeserializeOwned};
use std::io::Write;
use std::marker::PhantomData;
use std::os::unix::io::{AsRawFd, BorrowedFd, FromRawFd, RawFd};

#[derive(Debug)]
pub enum IpcStatus<T> {
    Ready(T),
    Empty,
    Disconnected,
}

#[derive(Debug)]
pub struct SubshellSender<T> {
    write_fd: RawFd,
    _marker: PhantomData<T>,
}

impl<T: Serialize> SubshellSender<T> {
    pub fn send(&self, payload: &T) -> bool {
        // Detach log streaming before writing the final payload to avoid interleaved log packets
        crate::logging::set_subshell_ipc_fd(None);

        match rmp_serde::to_vec_named(payload) {
            Ok(serialized) => {
                let len = (1 + serialized.len()) as u64;
                let tag: u8 = 1;
                let mut file = unsafe { std::fs::File::from_raw_fd(self.write_fd) };
                let write_res = file
                    .write_all(&len.to_ne_bytes())
                    .and_then(|_| file.write_all(&[tag]))
                    .and_then(|_| file.write_all(&serialized))
                    .and_then(|_| file.flush());

                std::mem::forget(file);

                write_res.is_ok()
            }
            Err(e) => {
                log::error!(
                    "SubshellIPC: serialization failed for write_fd {}: {:?}",
                    self.write_fd,
                    e
                );
                false
            }
        }
    }
}

impl<T> SubshellSender<T> {
    pub fn raw_fd(&self) -> RawFd {
        self.write_fd
    }
}

#[derive(Debug)]
pub struct SubshellReceiver<T> {
    read_fd: RawFd,
    _marker: PhantomData<T>,
}

impl<T: DeserializeOwned> SubshellReceiver<T> {
    pub fn poll_status(&self) -> IpcStatus<T> {
        loop {
            let mut fds = [PollFd::new(
                unsafe { BorrowedFd::borrow_raw(self.read_fd) },
                PollFlags::POLLIN | PollFlags::POLLHUP | PollFlags::POLLERR,
            )];

            match poll(&mut fds, PollTimeout::from(0u16)) {
                Ok(count) => {
                    if count == 0 {
                        return IpcStatus::Empty;
                    }

                    let revents = fds[0].revents().unwrap_or(PollFlags::empty());
                    if revents.contains(PollFlags::POLLERR) {
                        log::error!(
                            "SubshellIPC: pipe read_fd {} received POLLERR",
                            self.read_fd
                        );
                        return IpcStatus::Disconnected;
                    }
                    if revents.contains(PollFlags::POLLHUP) && !revents.contains(PollFlags::POLLIN)
                    {
                        log::warn!(
                            "SubshellIPC: pipe read_fd {} received POLLHUP (child disconnected)",
                            self.read_fd
                        );
                        return IpcStatus::Disconnected;
                    }

                    if revents.contains(PollFlags::POLLIN) {
                        let mut file = unsafe { std::fs::File::from_raw_fd(self.read_fd) };
                        let mut len_buf = [0u8; 8];

                        if let Err(e) = std::io::Read::read_exact(&mut file, &mut len_buf) {
                            std::mem::forget(file);
                            log::error!(
                                "SubshellIPC: read_exact for length header failed on read_fd {}: {:?}",
                                self.read_fd,
                                e
                            );
                            return IpcStatus::Disconnected;
                        }

                        let len = u64::from_ne_bytes(len_buf);
                        if len == 0 {
                            std::mem::forget(file);
                            log::error!(
                                "SubshellIPC: zero-length packet header received on read_fd {}",
                                self.read_fd
                            );
                            return IpcStatus::Disconnected;
                        }

                        let mut tag_buf = [0u8; 1];
                        if let Err(e) = std::io::Read::read_exact(&mut file, &mut tag_buf) {
                            std::mem::forget(file);
                            log::error!(
                                "SubshellIPC: read_exact for packet tag failed on read_fd {}: {:?}",
                                self.read_fd,
                                e
                            );
                            return IpcStatus::Disconnected;
                        }

                        let data_len = (len - 1) as usize;
                        let mut data_buf = vec![0u8; data_len];
                        if let Err(e) = std::io::Read::read_exact(&mut file, &mut data_buf) {
                            std::mem::forget(file);
                            log::error!(
                                "SubshellIPC: read_exact for packet data ({}) bytes failed on read_fd {}: {:?}",
                                data_len,
                                self.read_fd,
                                e
                            );
                            return IpcStatus::Disconnected;
                        }

                        std::mem::forget(file);

                        match tag_buf[0] {
                            0 => {
                                // Tag 0: Real-time streamed log line
                                if let Ok(log_entry) = String::from_utf8(data_buf) {
                                    crate::logging::log_raw_entry(log_entry);
                                }
                                // Continue reading remaining packets in this tick
                                continue;
                            }
                            1 => {
                                // Tag 1: Final payload
                                match rmp_serde::from_slice::<T>(&data_buf) {
                                    Ok(payload) => {
                                        log::info!(
                                            "SubshellIPC: successfully deserialized payload from read_fd {}",
                                            self.read_fd
                                        );
                                        return IpcStatus::Ready(payload);
                                    }
                                    Err(e) => {
                                        log::error!(
                                            "SubshellIPC: deserialization failed on read_fd {}: {:?}",
                                            self.read_fd,
                                            e
                                        );
                                        return IpcStatus::Disconnected;
                                    }
                                }
                            }
                            other => {
                                log::error!(
                                    "SubshellIPC: unrecognized packet tag {} on read_fd {}",
                                    other,
                                    self.read_fd
                                );
                                return IpcStatus::Disconnected;
                            }
                        }
                    } else {
                        return IpcStatus::Empty;
                    }
                }
                Err(e) => {
                    log::error!(
                        "SubshellIPC: poll failed on read_fd {}: {:?}",
                        self.read_fd,
                        e
                    );
                    return IpcStatus::Empty;
                }
            }
        }
    }
}

impl<T> SubshellReceiver<T> {
    pub fn raw_fd(&self) -> RawFd {
        self.read_fd
    }
}

#[derive(Debug)]
pub struct SubshellHandle<T> {
    pub pid: Pid,
    pub receiver: SubshellReceiver<T>,
}

impl<T> Drop for SubshellHandle<T> {
    fn drop(&mut self) {
        let _ = nix::sys::signal::kill(self.pid, nix::sys::signal::Signal::SIGKILL);
        let _ = nix::sys::wait::waitpid(self.pid, None);
        close_fd(self.receiver.raw_fd());
    }
}

pub fn channel<T>() -> Option<(SubshellSender<T>, SubshellReceiver<T>)> {
    match pipe() {
        Ok((read_pipe, write_pipe)) => {
            let read_fd = read_pipe.as_raw_fd();
            let write_fd = write_pipe.as_raw_fd();

            #[cfg(target_os = "linux")]
            unsafe {
                libc::fcntl(write_fd, libc::F_SETPIPE_SZ, 1_048_576);
                libc::fcntl(read_fd, libc::F_SETPIPE_SZ, 1_048_576);
            }

            std::mem::forget(read_pipe);
            std::mem::forget(write_pipe);

            log::info!(
                "SubshellIPC: created channel with read_fd={}, write_fd={}",
                read_fd,
                write_fd
            );

            Some((
                SubshellSender {
                    write_fd,
                    _marker: PhantomData,
                },
                SubshellReceiver {
                    read_fd,
                    _marker: PhantomData,
                },
            ))
        }
        Err(e) => {
            log::error!("SubshellIPC: failed to create nix pipe: {:?}", e);
            None
        }
    }
}

/// Spawns a dedicated subshell child process that communicates with the parent via an IPC channel.
///
/// In the child:
/// 1. Unused read FD is closed.
/// 2. `BASH_LOCK` is defensively reset.
/// 3. Process group is isolated (`setsid`).
/// 4. Standard terminal signals are reset to `SIG_DFL`.
/// 5. Stdin, stdout, and stderr are redirected to `/dev/null` to prevent terminal interference.
/// 6. Real-time log streaming over the IPC pipe is enabled.
/// 7. The `child_task` closure is executed.
/// 8. If `Some(payload)` is returned, the payload is serialized and sent to the parent.
/// 9. The child terminates cleanly via `libc::_exit(0)`.
///
/// In the parent:
/// 1. Unused write FD is closed.
/// 2. Returns `Some(SubshellHandle { pid, receiver })` or `None` on failure.
pub fn spawn_subshell<T: Serialize + DeserializeOwned, F>(
    child_task: F,
) -> Option<SubshellHandle<T>>
where
    F: FnOnce() -> Option<T>,
{
    let (tx, rx) = channel::<T>()?;

    match unsafe { fork() } {
        Ok(ForkResult::Child) => {
            close_fd(rx.raw_fd());

            unsafe {
                crate::bash_symbols::reset_bash_lock_after_fork();
                libc::setsid();
                for sig in &[
                    libc::SIGINT,
                    libc::SIGTERM,
                    libc::SIGHUP,
                    libc::SIGQUIT,
                    libc::SIGTSTP,
                    libc::SIGTTIN,
                    libc::SIGTTOU,
                ] {
                    libc::signal(*sig, libc::SIG_DFL);
                }
                let dev_null =
                    libc::open(b"/dev/null\0".as_ptr() as *const libc::c_char, libc::O_RDWR);
                if dev_null >= 0 {
                    libc::dup2(dev_null, libc::STDIN_FILENO);
                    libc::dup2(dev_null, libc::STDOUT_FILENO);
                    libc::dup2(dev_null, libc::STDERR_FILENO);
                    if dev_null > libc::STDERR_FILENO {
                        libc::close(dev_null);
                    }
                }
            }

            // this means we will send logs over to parent process in real-time
            // This helps enormously to debug child process issues.
            crate::logging::set_subshell_ipc_fd(Some(tx.raw_fd()));

            if let Some(payload) = child_task() {
                tx.send(&payload);
            }

            crate::logging::set_subshell_ipc_fd(None);

            unsafe {
                libc::_exit(0);
            }
        }
        Ok(ForkResult::Parent { child }) => {
            close_fd(tx.raw_fd());
            Some(SubshellHandle {
                pid: child,
                receiver: rx,
            })
        }
        Err(e) => {
            log::error!("SubshellIPC: fork failed: {:?}", e);
            None
        }
    }
}

pub fn close_fd(fd: RawFd) {
    log::info!("SubshellIPC: closing fd {}", fd);
    let _ = nix::unistd::close(fd);
}

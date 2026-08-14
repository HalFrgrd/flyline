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
    pub fn send(&self, value: &T) -> bool {
        match bincode::serialize(value) {
            Ok(serialized) => {
                let mut file = unsafe { std::fs::File::from_raw_fd(self.write_fd) };
                let len = serialized.len() as u64;
                log::info!(
                    "SubshellIPC: sending payload of {} bytes on write_fd {}",
                    len,
                    self.write_fd
                );
                let write_res = file
                    .write_all(&len.to_ne_bytes())
                    .and_then(|_| file.write_all(&serialized))
                    .and_then(|_| file.flush());

                std::mem::forget(file);

                match write_res {
                    Ok(_) => {
                        log::info!(
                            "SubshellIPC: successfully wrote payload to write_fd {}",
                            self.write_fd
                        );
                        true
                    }
                    Err(e) => {
                        log::error!(
                            "SubshellIPC: write_all/flush failed on write_fd {}: {:?}",
                            self.write_fd,
                            e
                        );
                        false
                    }
                }
            }
            Err(e) => {
                log::error!(
                    "SubshellIPC: bincode serialization failed for write_fd {}: {:?}",
                    self.write_fd,
                    e
                );
                false
            }
        }
    }

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
                if revents.contains(PollFlags::POLLHUP) && !revents.contains(PollFlags::POLLIN) {
                    log::warn!(
                        "SubshellIPC: pipe read_fd {} received POLLHUP (child disconnected)",
                        self.read_fd
                    );
                    return IpcStatus::Disconnected;
                }
                if revents.contains(PollFlags::POLLERR) {
                    log::error!(
                        "SubshellIPC: pipe read_fd {} received POLLERR",
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
                    log::info!(
                        "SubshellIPC: reading payload of {} bytes from read_fd {}",
                        len,
                        self.read_fd
                    );

                    let mut data_buf = vec![0u8; len as usize];
                    if let Err(e) = std::io::Read::read_exact(&mut file, &mut data_buf) {
                        std::mem::forget(file);
                        log::error!(
                            "SubshellIPC: read_exact for payload data ({}) bytes failed on read_fd {}: {:?}",
                            len,
                            self.read_fd,
                            e
                        );
                        return IpcStatus::Disconnected;
                    }

                    std::mem::forget(file);

                    match bincode::deserialize(&data_buf) {
                        Ok(payload) => {
                            log::info!(
                                "SubshellIPC: successfully deserialized payload from read_fd {}",
                                self.read_fd
                            );
                            IpcStatus::Ready(payload)
                        }
                        Err(e) => {
                            log::error!(
                                "SubshellIPC: bincode deserialization failed on read_fd {}: {:?}",
                                self.read_fd,
                                e
                            );
                            IpcStatus::Disconnected
                        }
                    }
                } else {
                    IpcStatus::Empty
                }
            }
            Err(e) => {
                log::error!(
                    "SubshellIPC: poll failed on read_fd {}: {:?}",
                    self.read_fd,
                    e
                );
                IpcStatus::Empty
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
/// 2. Process group is isolated (`setsid`).
/// 3. Standard terminal signals are reset to `SIG_DFL`.
/// 4. Stdin, stdout, and stderr are redirected to `/dev/null` to prevent terminal interference.
/// 5. The `child_task` closure is executed and its return value is serialized and sent to the parent.
/// 6. The child terminates cleanly via `libc::_exit(0)`.
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

            if let Some(payload) = child_task() {
                tx.send(&payload);
            }

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

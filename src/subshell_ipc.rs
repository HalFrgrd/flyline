use nix::poll::{PollFd, PollFlags, PollTimeout, poll};
use nix::unistd::{ForkResult, Pid, fork, pipe};
use serde::{Serialize, de::DeserializeOwned};
use std::marker::PhantomData;
use std::os::unix::io::{AsRawFd, BorrowedFd, RawFd};

#[derive(Debug)]
pub enum IpcStatus<T> {
    Ready(T),
    Empty,
    Disconnected,
}

#[derive(Debug)]
pub struct SubshellSender<T> {
    payload_tx: RawFd,
    _marker: PhantomData<T>,
}

pub(crate) fn write_all_fd(fd: RawFd, mut buf: &[u8]) -> std::io::Result<()> {
    while !buf.is_empty() {
        match nix::unistd::write(unsafe { BorrowedFd::borrow_raw(fd) }, buf) {
            Ok(0) => {
                return Err(std::io::Error::new(
                    std::io::ErrorKind::WriteZero,
                    "zero write",
                ));
            }
            Ok(n) => buf = &buf[n..],
            Err(nix::errno::Errno::EINTR) => continue,
            Err(e) => return Err(std::io::Error::from_raw_os_error(e as i32)),
        }
    }
    Ok(())
}

impl<T: Serialize> SubshellSender<T> {
    pub fn new(payload_tx: RawFd) -> Self {
        Self {
            payload_tx,
            _marker: PhantomData,
        }
    }

    pub fn send(&self, payload: &T) -> bool {
        match rmp_serde::to_vec_named(payload) {
            Ok(serialized) => {
                let len = serialized.len() as u64;
                write_all_fd(self.payload_tx, &len.to_ne_bytes())
                    .and_then(|_| write_all_fd(self.payload_tx, &serialized))
                    .is_ok()
            }
            Err(e) => {
                log::error!(
                    "SubshellIPC: serialization failed for payload_tx {}: {:?}",
                    self.payload_tx,
                    e
                );
                false
            }
        }
    }
}

#[derive(Debug)]
pub struct SubshellReceiver<T> {
    payload_rx: RawFd,
    log_rx: RawFd,
    _marker: PhantomData<T>,
}

impl<T> SubshellReceiver<T> {
    pub fn new(payload_rx: RawFd, log_rx: RawFd) -> Self {
        Self {
            payload_rx,
            log_rx,
            _marker: PhantomData,
        }
    }

    pub fn payload_fd(&self) -> RawFd {
        self.payload_rx
    }

    pub fn log_fd(&self) -> RawFd {
        self.log_rx
    }
}

const MAX_LOG_SIZE: usize = 1024 * 1024 * 1024; // 1 GB limit
const MAX_PAYLOAD_SIZE: usize = 1024 * 1024 * 1024; // 1 GB limit

fn read_exact_fd(fd: RawFd, mut buf: &mut [u8]) -> std::io::Result<()> {
    while !buf.is_empty() {
        match nix::unistd::read(fd, buf) {
            Ok(0) => {
                return Err(std::io::Error::new(
                    std::io::ErrorKind::UnexpectedEof,
                    "EOF",
                ));
            }
            Ok(n) => buf = &mut buf[n..],
            Err(nix::errno::Errno::EINTR) => continue,
            Err(e) => return Err(std::io::Error::from_raw_os_error(e as i32)),
        }
    }
    Ok(())
}

impl<T: DeserializeOwned> SubshellReceiver<T> {
    pub fn poll_status(&self) -> IpcStatus<T> {
        self.poll_status_timeout(0)
    }

    pub fn poll_status_timeout(&self, timeout_ms: u16) -> IpcStatus<T> {
        // 1. Drain all pending log packets from dedicated log pipe
        loop {
            let mut log_fds = [PollFd::new(
                unsafe { BorrowedFd::borrow_raw(self.log_rx) },
                PollFlags::POLLIN,
            )];

            match poll(&mut log_fds, PollTimeout::from(0u16)) {
                Ok(count) if count > 0 => {
                    let revents = log_fds[0].revents().unwrap_or(PollFlags::empty());
                    if !revents.contains(PollFlags::POLLIN) {
                        break;
                    }

                    let mut len_buf = [0u8; 8];
                    if read_exact_fd(self.log_rx, &mut len_buf).is_err() {
                        break;
                    }

                    let len = u64::from_ne_bytes(len_buf) as usize;
                    if len > MAX_LOG_SIZE {
                        log::error!("SubshellIPC: log packet size {} exceeds limit", len);
                        break;
                    }

                    let mut data_buf = vec![0u8; len];
                    if read_exact_fd(self.log_rx, &mut data_buf).is_err() {
                        break;
                    }

                    if let Ok(log_entry) = String::from_utf8(data_buf) {
                        crate::logging::log_raw_entry(log_entry);
                    }
                }
                _ => break,
            }
        }

        // 2. Check dedicated payload pipe for final result
        let mut payload_fds = [PollFd::new(
            unsafe { BorrowedFd::borrow_raw(self.payload_rx) },
            PollFlags::POLLIN | PollFlags::POLLHUP | PollFlags::POLLERR,
        )];

        match poll(&mut payload_fds, PollTimeout::from(timeout_ms)) {
            Ok(count) => {
                if count == 0 {
                    return IpcStatus::Empty;
                }

                let revents = payload_fds[0].revents().unwrap_or(PollFlags::empty());

                if revents.contains(PollFlags::POLLIN) {
                    let mut len_buf = [0u8; 8];
                    if let Err(e) = read_exact_fd(self.payload_rx, &mut len_buf) {
                        log::error!(
                            "SubshellIPC: read length failed on payload_rx {}: {:?}",
                            self.payload_rx,
                            e
                        );
                        return IpcStatus::Disconnected;
                    }

                    let len = u64::from_ne_bytes(len_buf) as usize;
                    if len > MAX_PAYLOAD_SIZE {
                        log::error!("SubshellIPC: payload size {} exceeds limit", len);
                        return IpcStatus::Disconnected;
                    }

                    let mut data_buf = vec![0u8; len];
                    if let Err(e) = read_exact_fd(self.payload_rx, &mut data_buf) {
                        log::error!(
                            "SubshellIPC: read data failed on payload_rx {}: {:?}",
                            self.payload_rx,
                            e
                        );
                        return IpcStatus::Disconnected;
                    }

                    match rmp_serde::from_slice::<T>(&data_buf) {
                        Ok(payload) => IpcStatus::Ready(payload),
                        Err(e) => {
                            log::error!(
                                "SubshellIPC: deserialization failed on payload_rx {}: {:?}",
                                self.payload_rx,
                                e
                            );
                            IpcStatus::Disconnected
                        }
                    }
                } else if revents.contains(PollFlags::POLLHUP)
                    || revents.contains(PollFlags::POLLERR)
                {
                    IpcStatus::Disconnected
                } else {
                    IpcStatus::Empty
                }
            }
            Err(e) => {
                log::error!(
                    "SubshellIPC: poll failed on payload_rx {}: {:?}",
                    self.payload_rx,
                    e
                );
                IpcStatus::Empty
            }
        }
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
        close_fd(self.receiver.payload_fd());
        close_fd(self.receiver.log_fd());
    }
}

fn create_pipe() -> Option<(RawFd, RawFd)> {
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

            log::trace!(
                "SubshellIPC: created pipe with read_fd={}, write_fd={}",
                read_fd,
                write_fd
            );

            Some((read_fd, write_fd))
        }
        Err(e) => {
            log::error!("SubshellIPC: failed to create nix pipe: {:?}", e);
            None
        }
    }
}

/// Spawns a dedicated subshell child process that communicates with the parent via two dedicated IPC channels:
/// 1. `log_pipe`: Streams debug/diagnostic logs from child to parent in real-time.
/// 2. `payload_pipe`: Delivers the final strongly-typed result payload.
pub fn spawn_subshell<T: Serialize + DeserializeOwned, F>(
    child_task: F,
) -> Option<SubshellHandle<T>>
where
    F: FnOnce() -> Option<T>,
{
    let (payload_rx, payload_tx) = create_pipe()?;
    let (log_rx, log_tx) = match create_pipe() {
        Some(pipes) => pipes,
        None => {
            close_fd(payload_rx);
            close_fd(payload_tx);
            return None;
        }
    };

    match unsafe { fork() } {
        Ok(ForkResult::Child) => {
            close_fd(payload_rx);
            close_fd(log_rx);

            unsafe {
                #[cfg(not(test))]
                crate::shell::bash::symbols::reset_bash_lock_after_fork();
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
                let dev_null = libc::open(c"/dev/null".as_ptr(), libc::O_RDWR);
                if dev_null >= 0 {
                    libc::dup2(dev_null, libc::STDIN_FILENO);
                    libc::dup2(dev_null, libc::STDOUT_FILENO);
                    libc::dup2(dev_null, libc::STDERR_FILENO);
                    if dev_null > libc::STDERR_FILENO {
                        libc::close(dev_null);
                    }
                }
            }

            // Real-time log streaming over the dedicated log pipe
            crate::logging::set_subshell_ipc_fd(Some(log_tx));

            let tx = SubshellSender::new(payload_tx);

            if let Some(payload) = child_task() {
                tx.send(&payload);
            }

            crate::logging::set_subshell_ipc_fd(None);

            close_fd(payload_tx);
            close_fd(log_tx);

            unsafe {
                libc::_exit(0);
            }
        }
        Ok(ForkResult::Parent { child }) => {
            close_fd(payload_tx);
            close_fd(log_tx);
            Some(SubshellHandle {
                pid: child,
                receiver: SubshellReceiver::new(payload_rx, log_rx),
            })
        }
        Err(e) => {
            log::error!("SubshellIPC: fork failed: {:?}", e);
            close_fd(payload_rx);
            close_fd(payload_tx);
            close_fd(log_rx);
            close_fd(log_tx);
            None
        }
    }
}

pub fn close_fd(fd: RawFd) {
    log::trace!("SubshellIPC: closing fd {}", fd);
    let _ = nix::unistd::close(fd);
}

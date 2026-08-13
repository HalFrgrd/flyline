use serde::{Serialize, de::DeserializeOwned};
use std::marker::PhantomData;
use std::os::unix::io::{AsRawFd, BorrowedFd, FromRawFd, RawFd};

#[derive(Debug)]
pub struct SubshellSender<T> {
    write_fd: RawFd,
    _marker: PhantomData<T>,
}

impl<T: Serialize> SubshellSender<T> {
    pub fn send(&self, value: &T) -> bool {
        if let Ok(serialized) = bincode::serialize(value) {
            let mut file = unsafe { std::fs::File::from_raw_fd(self.write_fd) };
            use std::io::Write;
            let len = serialized.len() as u64;
            let success =
                file.write_all(&len.to_ne_bytes()).is_ok() && file.write_all(&serialized).is_ok();
            let _ = file.flush();
            std::mem::forget(file);
            return success;
        }
        false
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
    pub fn try_recv(&self) -> Option<T> {
        use nix::poll::{PollFd, PollFlags, PollTimeout, poll};
        let mut fds = [PollFd::new(
            unsafe { BorrowedFd::borrow_raw(self.read_fd) },
            PollFlags::POLLIN,
        )];

        if let Ok(count) = poll(&mut fds, PollTimeout::from(0u16)) {
            if count > 0 {
                let mut file = unsafe { std::fs::File::from_raw_fd(self.read_fd) };
                let mut len_buf = [0u8; 8];
                let res = if std::io::Read::read_exact(&mut file, &mut len_buf).is_ok() {
                    let len = u64::from_ne_bytes(len_buf);
                    let mut data_buf = vec![0u8; len as usize];
                    if std::io::Read::read_exact(&mut file, &mut data_buf).is_ok() {
                        bincode::deserialize(&data_buf).ok()
                    } else {
                        None
                    }
                } else {
                    None
                };
                std::mem::forget(file);
                return res;
            }
        }
        None
    }

    pub fn raw_fd(&self) -> RawFd {
        self.read_fd
    }
}

pub fn channel<T>() -> Option<(SubshellSender<T>, SubshellReceiver<T>)> {
    use nix::unistd::pipe;
    let (read_pipe, write_pipe) = pipe().ok()?;
    let read_fd = read_pipe.as_raw_fd();
    let write_fd = write_pipe.as_raw_fd();
    std::mem::forget(read_pipe);
    std::mem::forget(write_pipe);

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

pub fn close_fd(fd: RawFd) {
    let _ = nix::unistd::close(fd);
}

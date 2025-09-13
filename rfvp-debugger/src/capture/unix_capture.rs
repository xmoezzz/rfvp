use super::StdoutCapture;
use crossbeam_channel::Sender;
use anyhow::Result;
use nix::unistd::{dup, dup2, pipe, close};
use std::{fs::File, io::Read, thread};
use std::os::fd::FromRawFd;

pub struct UnixCapture {
    orig_out: i32,
    orig_err: i32,
    reader_thread: thread::JoinHandle<()>,
}

impl StdoutCapture for UnixCapture {
    fn start(log_tx: Sender<String>) -> Result<Self> {
        let (reader_fd, writer_fd) = pipe()?;
        let orig_out = dup(libc::STDOUT_FILENO)?;
        let orig_err = dup(libc::STDERR_FILENO)?;
        dup2(writer_fd, libc::STDOUT_FILENO)?;
        dup2(writer_fd, libc::STDERR_FILENO)?;
        close(writer_fd)?;

        let handle = thread::spawn(move || {
            let mut reader = unsafe { File::from_raw_fd(reader_fd) };
            let mut buf = [0u8; 4096];
            while let Ok(n) = reader.read(&mut buf) {
                if n == 0 { break; }
                if let Ok(s) = std::str::from_utf8(&buf[..n]) {
                    let _ = log_tx.send(s.to_string());
                }
            }
        });

        Ok(UnixCapture { orig_out, orig_err, reader_thread: handle })
    }

    fn stop(self) -> Result<()> {
        dup2(self.orig_out, libc::STDOUT_FILENO)?;
        dup2(self.orig_err, libc::STDERR_FILENO)?;
        close(self.orig_out)?;
        close(self.orig_err)?;
        let _ = self.reader_thread.join();
        Ok(())
    }
}


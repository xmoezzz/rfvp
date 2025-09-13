use super::StdoutCapture;
use crossbeam_channel::Sender;
use anyhow::Result;
use std::{ptr, thread, io::Read};
use windows_sys::Win32::{
    Foundation::*,
    Storage::FileSystem::*,
    System::Console::*,
};

pub struct WinCapture {
    orig_out: HANDLE,
    orig_err: HANDLE,
    reader_thread: thread::JoinHandle<()>,
}

impl StdoutCapture for WinCapture {
    fn start(log_tx: Sender<String>) -> Result<Self> {
        unsafe {
            let mut read_pipe: HANDLE = 0;
            let mut write_pipe: HANDLE = 0;
            if CreatePipe(&mut read_pipe, &mut write_pipe, ptr::null_mut(), 0) == 0 {
                return Err(anyhow::anyhow!("CreatePipe failed"));
            }
            let orig_out = GetStdHandle(STD_OUTPUT_HANDLE);
            let orig_err = GetStdHandle(STD_ERROR_HANDLE);
            SetStdHandle(STD_OUTPUT_HANDLE, write_pipe);
            SetStdHandle(STD_ERROR_HANDLE, write_pipe);

            let handle = thread::spawn(move || {
                let mut f = unsafe { std::fs::File::from_raw_handle(read_pipe as _) };
                let mut buf = [0u8; 4096];
                while let Ok(n) = f.read(&mut buf) {
                    if n == 0 { break; }
                    if let Ok(s) = std::str::from_utf8(&buf[..n]) {
                        let _ = log_tx.send(s.to_string());
                    }
                }
            });

            Ok(WinCapture { orig_out, orig_err, reader_thread: handle })
        }
    }

    fn stop(self) -> Result<()> {
        unsafe {
            SetStdHandle(STD_OUTPUT_HANDLE, self.orig_out);
            SetStdHandle(STD_ERROR_HANDLE, self.orig_err);
        }
        let _ = self.reader_thread.join();
        Ok(())
    }
}

//! Sync/waiting utilities and wrappers

use crate::arm;
use crate::result::*;
use crate::svc;

/// Represents an event via a remote handle
pub struct RemoteEvent {
    /// The remote handle
    pub handle: svc::Handle,
}

impl RemoteEvent {
    /// Creates a [`RemoteEvent`] from a remote handle
    ///
    /// # Arguments
    ///
    /// * `handle` - The remote handle
    #[inline]
    pub const fn new(handle: svc::Handle) -> Self {
        Self { handle }
    }

    /// Resets the [`RemoteEvent`]
    #[inline]
    pub fn reset(&self) -> Result<()> {
        svc::reset_signal(self.handle)
    }

    /// Waits for the [`RemoteEvent`] with a given timeout, then resets it
    ///
    /// # Arguments
    ///
    /// * `timeout` - Wait timeout in nanoseconds, `-1` can be used to wait indefinitely
    #[inline]
    pub fn wait(&self, timeout: i64) -> Result<()> {
        wait_handles(&[self.handle], timeout)?;
        self.reset()
    }
}

impl Drop for RemoteEvent {
    /// Destroys the [`RemoteEvent`], closing its handle
    fn drop(&mut self) {
        let _ = svc::close_handle(self.handle);
    }
}

/// Represents a system event with server and client handles
pub struct SystemEvent {
    /// The event's server handle
    pub server_handle: svc::Handle,
    /// The event's client handle
    pub client_handle: svc::Handle,
}

impl SystemEvent {
    /// Creates a new [`SystemEvent`] via the client/server handles obtained from [`svc::create_event`]
    ///
    /// # Arguments
    ///
    /// * `timeout` - Wait timeout in nanoseconds, `-1` can be used to wait indefinitely
    pub fn new() -> Result<Self> {
        let (server_handle, client_handle) = svc::create_event()?;
        Ok(Self {
            server_handle,
            client_handle,
        })
    }

    /// Signals the [`SystemEvent`] (via the server handle)
    #[inline]
    pub fn signal(&self) -> Result<()> {
        svc::signal_event(self.server_handle)
    }
}

impl Drop for SystemEvent {
    /// Destroys the [`SystemEvent`], closing both server/client handles
    fn drop(&mut self) {
        let _ = svc::close_handle(self.server_handle);
        let _ = svc::close_handle(self.client_handle);
    }
}

/// Represents how a waiter operates (essentially, whether it gets automatically cleared after being signaled)
pub enum WaiterType {
    /// A simple handle, that doesn't get cleared when the waiter wakes
    Handle,
    /// A wait handle that has the signal automatically cleared
    HandleWithClear,
}

/// Represents the max amount of objects the Nintendo Switch kernel can wait-sync on at the same time (like Windows)
pub const MAX_OBJECT_COUNT: u32 = 0x40;

/// Represents a waiting object for a handle
#[allow(dead_code)]
pub struct Waiter {
    handle: svc::Handle,
    wait_type: WaiterType,
}

impl Waiter {
    /// Creates a new [`Waiter`] from a handle and a type
    ///
    /// # Arguments
    ///
    /// * `handle` - The waiter handle
    /// * `wait_type` - Thr waiter type
    #[inline]
    pub const fn from(handle: svc::Handle, wait_type: WaiterType) -> Self {
        Self { handle, wait_type }
    }

    /// Creates a new [`Waiter`] from a handle and [`WaiterType::Handle`] type
    ///
    /// # Arguments
    ///
    /// * `handle` - The waiter handle
    #[inline]
    pub const fn from_handle(handle: svc::Handle) -> Self {
        Self::from(handle, WaiterType::Handle)
    }

    /// Creates a new `Waiter` from a handle and [`WaiterType::HandleWithClear`] type
    ///
    /// # Arguments
    ///
    /// * `handle` - The waiter handle
    #[inline]
    pub const fn from_handle_with_clear(handle: svc::Handle) -> Self {
        Self::from(handle, WaiterType::HandleWithClear)
    }
}

type WaitFn<W> = fn(&[W], i64) -> Result<usize>;

fn handles_wait_fn(handles: &[svc::Handle], timeout: i64) -> Result<usize> {
    unsafe { svc::wait_synchronization(handles, timeout).map(|idx| idx as usize) }
}

fn waiters_wait_fn(_waiters: &[Waiter], _timeout: i64) -> Result<usize> {
    todo!();
}

fn wait_impl<W>(wait_objects: &[W], timeout: i64, wait_fn: WaitFn<W>) -> Result<usize> {
    let has_timeout = timeout != -1;
    let mut deadline: u64 = 0;
    if has_timeout {
        deadline = arm::get_system_tick().saturating_add(arm::nanoseconds_to_ticks(timeout as u64));
    }

    loop {
        let this_timeout = match has_timeout {
            true => {
                let remaining = deadline.saturating_sub(arm::get_system_tick());
                arm::ticks_to_nanoseconds(remaining) as i64
            }
            false => -1,
        };
        match (wait_fn)(wait_objects, this_timeout) {
            Ok(index) => return Ok(index),
            Err(rc) => {
                if svc::rc::ResultTimedOut::matches(rc) {
                    if has_timeout {
                        return Err(rc);
                    }
                } else if !svc::rc::ResultCancelled::matches(rc) {
                    return Err(rc);
                }
            }
        }
    }
}

/// Waits for several [`Waiter`]s for a specified timeout, returning the index of the waiter which signals first
///
/// # Arguments
///
/// * `waiters` - [`Waiter`]s to wait for
/// * `timeout` - Wait timeout in nanoseconds, `-1` can be used to wait indefinitely
#[inline]
pub fn wait(waiters: &[Waiter], timeout: i64) -> Result<usize> {
    wait_impl(waiters, timeout, waiters_wait_fn)
}

/// Waits for several handles for a specified timeout, returning the index of the handle which signals first
///
/// # Arguments
///
/// * `handles` - Handles to wait for
/// * `timeout` - Wait timeout in nanoseconds, `-1` can be used to wait indefinitely
#[inline]
pub fn wait_handles(handles: &[svc::Handle], timeout: i64) -> Result<usize> {
    wait_impl(handles, timeout, handles_wait_fn)
}

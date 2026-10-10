//! Implementation of the Rust libstd TCP/UDP APIs, and re-exports of the raw bsd sockets API.

use core::alloc::Layout;
use core::ops::Deref;
use core::ops::DerefMut;
use core::sync::atomic::AtomicU16;
use core::sync::atomic::AtomicUsize;
use core::sync::atomic::Ordering;

use alloc::alloc::Allocator;
use alloc::alloc::Global;
use alloc::boxed::Box;
use alloc::vec::Vec;

use crate::ipc::client::IClientObject;
use crate::ipc::sf;
use crate::ipc::sf::CopyHandle;
use crate::mem::alloc::Buffer;
use crate::mem::alloc::PAGE_ALIGNMENT;
use crate::mem::wait_for_permission;
use crate::result::Result;

pub use crate::service::bsd::*;

use crate::service::new_service_object;
use crate::svc::Handle;
use crate::svc::MemoryPermission;
use crate::sync::sys::futex::Futex;
use crate::sync::{ReadGuard, RwLock, WriteGuard};

#[repr(usize)]
pub enum Paralellism {
    One = 1,
    Two,
    Three,
    Four,
    Five,
    Six,
    Seven,
    Eight,
    Nine,
    Ten,
    Eleven,
    Twelve,
    Thirteen,
    Fourteen,
    Fifteen,
    Sixteen,
}
pub(crate) enum BsdServiceDispatcher {
    U(UserBsdService),
    A(AppletBsdService),
    S(SystemBsdService),
}

impl Deref for BsdServiceDispatcher {
    type Target = dyn IBsdClient;
    fn deref(&self) -> &Self::Target {
        match self {
            Self::A(service) => service as &dyn IBsdClient,
            Self::U(service) => service as &dyn IBsdClient,
            Self::S(service) => service as &dyn IBsdClient,
        }
    }
}

impl DerefMut for BsdServiceDispatcher {
    fn deref_mut(&mut self) -> &mut Self::Target {
        match self {
            Self::A(service) => service as &mut dyn IBsdClient,
            Self::U(service) => service as &mut dyn IBsdClient,
            Self::S(service) => service as &mut dyn IBsdClient,
        }
    }
}

struct BsdServiceHandle<'p> {
    parent: &'p BsdSocketService,
    slot: usize,
}

impl Deref for BsdServiceHandle<'_> {
    type Target = dyn IBsdClient;
    fn deref(&self) -> &Self::Target {
        unsafe { self.parent.services.get_unchecked(self.slot) }.deref()
    }
}

impl Drop for BsdServiceHandle<'_> {
    fn drop(&mut self) {
        let inverse_mask: u16 = !(1 << self.slot);
        // return the slot to the queue for use
        self.parent
            .checkout_slots
            .fetch_and(inverse_mask, Ordering::Release);

        // if there are any waiters, notify one
        if self.parent.waiters.load(Ordering::Acquire) > 0 {
            self.parent.service_waiter.signal_one();
        }
    }
}

/// Holder type for the intialized bsd service
pub struct BsdSocketService {
    _tmem_buffer: Buffer<u8>,
    tmem_handle: Handle,
    checkout_slots: AtomicU16,
    waiters: AtomicUsize,
    service_waiter: Futex,
    services: Vec<BsdServiceDispatcher>,
    _monitor_service: Box<dyn IBsdClient + Send + 'static>,
    _bsd_client_pid: u64,
}

unsafe impl Sync for BsdSocketService {}
unsafe impl Send for BsdSocketService {}

impl BsdSocketService {
    fn new(
        config: BsdServiceConfig,
        kind: BsdSrvkind,
        transfer_mem_buffer: Option<Buffer<u8>>,
        parrallellism: Paralellism,
    ) -> Result<Self> {
        let mut services: Vec<BsdServiceDispatcher> = match kind {
            BsdSrvkind::Applet => {
                let mut services = vec![new_service_object::<AppletBsdService>()?];
                for _ in 1..(parrallellism as usize) {
                    let copy = services[0].clone()?;
                    services.push(copy);
                }
                services.into_iter().map(BsdServiceDispatcher::A).collect()
            }
            BsdSrvkind::System => {
                let mut services = vec![new_service_object::<SystemBsdService>()?];
                for _ in 1..(parrallellism as usize) {
                    let copy = services[0].clone()?;
                    services.push(copy);
                }
                services.into_iter().map(BsdServiceDispatcher::S).collect()
            }
            BsdSrvkind::User => {
                let mut services = vec![new_service_object::<UserBsdService>()?];
                for _ in 1..(parrallellism as usize) {
                    let copy = services[0].clone()?;
                    services.push(copy);
                }
                services.into_iter().map(BsdServiceDispatcher::U).collect()
            }
        };

        let mut monitor_service = match kind {
            BsdSrvkind::Applet => Box::new(new_service_object::<AppletBsdService>()?)
                as Box<dyn IBsdClient + Send + Sync + 'static>,
            BsdSrvkind::System => Box::new(new_service_object::<SystemBsdService>()?)
                as Box<dyn IBsdClient + Send + Sync + 'static>,
            BsdSrvkind::User => Box::new(new_service_object::<UserBsdService>()?)
                as Box<dyn IBsdClient + Send + Sync + 'static>,
        };

        let tmem_min_size = config.min_transfer_mem_size();

        let tmem_buffer: Buffer<u8> = if let Some(prepared_buffer) = transfer_mem_buffer
            && prepared_buffer.layout.size() >= tmem_min_size
        {
            prepared_buffer
        } else {
            let layout =
                unsafe { Layout::from_size_align_unchecked(tmem_min_size, PAGE_ALIGNMENT) };
            Buffer {
                ptr: Global.allocate(layout).unwrap().as_ptr().cast(),
                layout,
                allocator: Global,
            }
        };

        let tmem_handle = crate::svc::create_transfer_memory(
            tmem_buffer.ptr.cast(),
            tmem_buffer.layout.size(),
            MemoryPermission::None(),
        )?;

        let bsd_client_pid = services[0].register_client(
            config,
            sf::ProcessId::new(),
            tmem_buffer.layout.size(),
            CopyHandle::from(tmem_handle),
        )?;

        monitor_service.start_monitoring(sf::ProcessId::from(bsd_client_pid))?;

        Ok(Self {
            _tmem_buffer: tmem_buffer,
            tmem_handle,
            checkout_slots: AtomicU16::new(0),
            waiters: AtomicUsize::new(0),
            service_waiter: Futex::new(),
            services,
            _monitor_service: monitor_service,
            _bsd_client_pid: bsd_client_pid,
        })
    }

    fn get_service(&self) -> BsdServiceHandle<'_> {
        let slot_limit = self.services.len();
        loop {
            if let Ok(value) =
                self.checkout_slots
                    .fetch_update(Ordering::AcqRel, Ordering::Acquire, |v| {
                        let slot = v.trailing_ones() as usize;
                        if slot < slot_limit {
                            // write a checkout bit into the checkout slot
                            Some(v | (1 << slot))
                        } else {
                            // all valid slots are filled, bail on the update so we can wait on the futex.
                            None
                        }
                    })
            {
                // since the atomic update succeeded, we can rerun the trailing_ones call on
                // the returned original value to re-calculate the slot we took.
                return BsdServiceHandle {
                    parent: self,
                    slot: value.trailing_ones() as usize,
                };
            } else {
                // wait for an active hold on the service to be released.
                self.waiters.fetch_add(1, Ordering::Release);
                self.service_waiter.wait(0, -1);
                self.waiters.fetch_sub(1, Ordering::Release);
            }
        }
    }
}

impl Drop for BsdSocketService {
    fn drop(&mut self) {
        self._monitor_service.close_session();
        self.services.iter_mut().for_each(drop);
        let _ = crate::svc::close_handle(self.tmem_handle);
        let _ = wait_for_permission(self._tmem_buffer.ptr as _, MemoryPermission::Write(), None);
    }
}

static BSD_SERVICE: RwLock<Option<BsdSocketService>> = RwLock::new(None);

/// Initializes the bsd/socket service.
pub fn initialize(
    kind: BsdSrvkind,
    config: BsdServiceConfig,
    tmem_buffer: Option<Buffer<u8>>,
    paralellism: Paralellism,
) -> Result<()> {
    let mut service_handle = BSD_SERVICE.write();

    if service_handle.is_some() {
        return Ok(());
    }

    *service_handle = Some(BsdSocketService::new(
        config,
        kind,
        tmem_buffer,
        paralellism,
    )?);

    Ok(())
}

pub(crate) fn finalize() {
    *BSD_SERVICE.write() = None;
}

pub fn read_socket_service<'a>() -> ReadGuard<'a, Option<BsdSocketService>> {
    BSD_SERVICE.read()
}

pub fn write_socket_service<'a>() -> WriteGuard<'a, Option<BsdSocketService>> {
    BSD_SERVICE.write()
}

/// Implementation of the Rust stdlib TCP/UDP API
pub mod net {
    use core::time::Duration;
    use core::{mem::offset_of, net::Ipv4Addr};

    use alloc::vec::Vec;

    use super::*;
    use crate::ipc::sf::OutAutoSelectBuffer;
    use crate::service::bsd::PollFlags;
    use crate::service::bsd::{PollFd, SocketOptions};
    use crate::socket::{BsdDuration, Linger, SOL_SOCKET};
    use crate::{
        ipc::sf::Buffer,
        result::{Result, ResultBase, ResultCode},
        service::bsd::{BsdResult, ReadFlags, SendFlags, SocketAddrRepr},
    };

    pub mod traits {
        use super::*;

        /// Trait for making a bsd-fd wrapper type usable in `super::poll`
        pub trait Pollable {
            fn get_poll_fd(&self) -> i32;
        }

        impl<T: SocketCommon> Pollable for T {
            fn get_poll_fd(&self) -> i32 {
                self.as_raw_fd()
            }
        }

        /// Contains common functions for bsd-compatible socket-like types
        ///
        /// # Safety
        ///
        /// Implementors are responsible for synchonising any interior mutablility for types, if any exists.
        pub unsafe trait SocketCommon {
            /// gets the raw file descriptor for the type
            fn as_raw_fd(&self) -> i32;

            /// Opens a connection to a remote host.
            fn connect<A: Into<Ipv4Addr>>(destination: A, port: u16) -> Result<Self>
            where
                Self: Sized;

            /// Creates a new independently owned handle to the underlying socket.
            ///
            /// The returned object is a references the same stream that this
            /// object references. Both handles will read and write the same stream of
            /// data, and options set on one stream will be propagated to the other
            /// stream.
            ///
            /// This function is also why objects implementing this trait _should not_ contain any methods requiring mutable references.
            /// Consumers should expect that calls to these functions are synchronized by the implementation.
            fn try_clone(&self) -> Result<Self>
            where
                Self: Sized;

            /// Reads data from the remote side into the provided buffer.
            ///
            /// Immediately returns an error if the socket is not connected.
            fn recv(&self, data: &mut [u8]) -> Result<usize> {
                let socket_server_handle = BSD_SERVICE.read();
                let socket_server = socket_server_handle.as_ref().unwrap();

                match socket_server.get_service().recv(
                    self.as_raw_fd(),
                    ReadFlags::None(),
                    Buffer::from_mut_array(data),
                )? {
                    BsdResult::Ok(ret, ()) => Ok(ret as usize),
                    BsdResult::Err(errno) => ResultCode::new_err(nx::result::pack_value(
                        rc::RESULT_MODULE,
                        1000 + errno.cast_unsigned(),
                    )),
                }
            }

            /// Receives single datagram on the socket from the remote address to which it is connected, without removing the message from input queue. On success, returns the number of bytes peeked.
            ///
            /// The function must be called with valid byte array buf of sufficient size to hold the message bytes. If a message is too long to fit in the supplied buffer, excess bytes may be discarded.
            ///
            /// Successive calls return the same data. This is accomplished by passing `MSG_PEEK`` as a flag to the underlying `recv` system call.
            ///
            /// Do not use this function to implement busy waiting, instead use [`poll`][`nx::socket::net::poll`] to synchronize IO events on one or more sockets.
            /// `UdpSocket::connect` will connect this socket to a remote address. This method will fail if the socket is not connected.
            ///
            /// # Errors
            ///
            /// This method will fail if the socket is not connected. The connect method will connect this socket to a remote address.
            fn peek(&self, data: &mut [u8]) -> Result<usize> {
                let socket_server_handle = BSD_SERVICE.read();
                let socket_server = socket_server_handle.as_ref().unwrap();

                match socket_server.get_service().recv(
                    self.as_raw_fd(),
                    ReadFlags::Peek(),
                    Buffer::from_mut_array(data),
                )? {
                    BsdResult::Ok(ret, ()) => Ok(ret as usize),
                    BsdResult::Err(errno) => ResultCode::new_err(nx::result::pack_value(
                        rc::RESULT_MODULE,
                        1000 + errno.cast_unsigned(),
                    )),
                }
            }

            /// Sends data on the socket to the remote address to which it is connected.
            /// For TCP, all data is sent or an error is returned.
            ///
            /// Returns the length of the data written from the buffer
            ///
            /// `Self::connect`` will connect this socket to a remote address. This method will fail if the socket is not connected.
            fn send(&self, data: &[u8]) -> Result<u32> {
                let socket_server_handle = BSD_SERVICE.read();
                let socket_server = socket_server_handle.as_ref().unwrap();

                match socket_server.get_service().send(
                    self.as_raw_fd(),
                    SendFlags::None(),
                    Buffer::from_array(data),
                )? {
                    BsdResult::Ok(len, ()) => Ok(len.cast_unsigned()),
                    BsdResult::Err(errno) => ResultCode::new_err(nx::result::pack_value(
                        rc::RESULT_MODULE,
                        1000 + errno.cast_unsigned(),
                    )),
                }
            }

            /// Sends data on the socket to the remote address to which it is connected.
            /// For TCP, all data is sent or an error is returned.
            ///
            /// Returns the length of the data written from the buffer
            ///
            /// `Self::connect`` will connect this socket to a remote address. This method will fail if the socket is not connected.
            fn send_non_blocking(&self, data: &[u8]) -> Result<()> {
                let socket_server_handle = BSD_SERVICE.read();
                let socket_server = socket_server_handle.as_ref().unwrap();

                match socket_server.get_service().send(
                    self.as_raw_fd(),
                    SendFlags::DontWait(),
                    Buffer::from_array(data),
                )? {
                    BsdResult::Ok(_, ()) => Ok(()),
                    BsdResult::Err(errno) => ResultCode::new_err(nx::result::pack_value(
                        rc::RESULT_MODULE,
                        1000 + errno.cast_unsigned(),
                    )),
                }
            }

            /// Receives data on the socket from the remote address to which it is connected.
            /// On success, returns the number of bytes read or a None value if there is no data.
            ///
            ///The function must be called with valid byte array buf of sufficient size to hold the message bytes. If a message is too long to fit in the supplied buffer, excess bytes may be discarded.
            ///
            /// `UdpSocket::connect`` will connect this socket to a remote address. This method will fail if the socket is not connected.
            fn recv_non_blocking(&self, buffer: &mut [u8]) -> Result<Option<usize>> {
                let socket_server_handle = BSD_SERVICE.read();
                let socket_server = socket_server_handle.as_ref().unwrap();

                match socket_server.get_service().recv(self.as_raw_fd(), ReadFlags::DontWait(), Buffer::from_mut_array(buffer))? {
                    BsdResult::Ok(ret, ()) => {
                        Ok(Some(ret as usize))
                    },
                    BsdResult::Err(11) /* EAGAIN */ => {
                        Ok(None)
                    }
                    BsdResult::Err(errno) => {
                        ResultCode::new_err(nx::result::pack_value(
                            rc::RESULT_MODULE,
                            1000 + errno.cast_unsigned(),
                        ))
                    }
                }
            }

            /// Returns the local address of this socket
            fn local_addr(&self) -> Result<SocketAddrRepr> {
                let socket_server_handle = BSD_SERVICE.read();

                let socket_server = socket_server_handle.as_ref().unwrap();

                let mut out_ip: SocketAddrRepr = Default::default();
                match socket_server
                    .get_service()
                    .get_socket_name(self.as_raw_fd(), Buffer::from_mut_var(&mut out_ip))?
                {
                    BsdResult::Ok(_, written_sockaddr_size) => {
                        debug_assert!(
                            written_sockaddr_size as usize >= offset_of!(SocketAddrRepr, _zero),
                            "Invalid write length for returned socket addr"
                        );
                        Ok(out_ip)
                    }
                    BsdResult::Err(errno) => ResultCode::new_err(nx::result::pack_value(
                        rc::RESULT_MODULE,
                        1000 + errno.cast_unsigned(),
                    )),
                }
            }

            /// Returns the remote address of this socket (errors for unconnected UDP sockets).
            fn peer_addr(&self) -> Result<SocketAddrRepr> {
                let socket_server_handle = BSD_SERVICE.read();
                let socket_server = socket_server_handle.as_ref().unwrap();

                let mut out_ip: SocketAddrRepr = Default::default();
                match socket_server
                    .get_service()
                    .get_peer_name(self.as_raw_fd(), Buffer::from_mut_var(&mut out_ip))?
                {
                    BsdResult::Ok(_, written_sockaddr_size) => {
                        debug_assert!(
                            written_sockaddr_size as usize >= offset_of!(SocketAddrRepr, _zero),
                            "Invalid write length for returned socket addr"
                        );
                        Ok(out_ip)
                    }
                    BsdResult::Err(errno) => ResultCode::new_err(nx::result::pack_value(
                        rc::RESULT_MODULE,
                        1000 + errno.cast_unsigned(),
                    )),
                }
            }

            /// Sets the value for the `IP_TTL` option on this socket.
            ///
            /// This value sets the time-to-live field that is used in every packet sent
            /// from this socket.
            fn set_ttl(&self, ttl: u32) -> Result<()> {
                let socket_server_handle = BSD_SERVICE.read();
                let socket_server = socket_server_handle.as_ref().unwrap();

                if let BsdResult::Err(errno) = socket_server.get_service().set_sock_opt(
                    self.as_raw_fd(),
                    IpProto::IP as _,
                    IpOptions::TimeToLive as _,
                    Buffer::from_other_var(&ttl),
                )? {
                    return ResultCode::new_err(nx::result::pack_value(
                        rc::RESULT_MODULE,
                        1000 + errno.cast_unsigned(),
                    ));
                }

                Ok(())
            }

            /// Gets the value of the `IP_TTL` option for this socket
            fn ttl(&self) -> Result<u32> {
                let socket_server_handle = BSD_SERVICE.read();
                let socket_server = socket_server_handle.as_ref().unwrap();

                let mut ttl: u32 = 0;
                if let BsdResult::Err(errno) = socket_server.get_service().get_sock_opt(
                    self.as_raw_fd(),
                    IpProto::IP as _,
                    IpOptions::TimeToLive as _,
                    Buffer::from_other_mut_var(&mut ttl),
                )? {
                    return ResultCode::new_err(nx::result::pack_value(
                        rc::RESULT_MODULE,
                        1000 + errno.cast_unsigned(),
                    ));
                }

                Ok(ttl)
            }

            /// Moves this TCP stream into or out of nonblocking mode.
            ///
            ///  This will result in `read`, `write`, `recv` and `send` system operations
            ///  becoming nonblocking, i.e., immediately returning from their calls.
            /// If the IO operation is successful, `Ok` is returned and no further
            /// action is required. If the IO operation could not be completed and needs
            /// to be retried, an error with the value set to `EAGAIN` is
            /// returned.
            fn set_nonblocking(&self, nonblocking: bool) -> Result<()> {
                const O_NONBLOCK: i32 = 0x800;

                let socket_server_handle = BSD_SERVICE.read();
                let socket_server = socket_server_handle.as_ref().unwrap();

                let current_flags = match socket_server.get_service().fcntl(
                    self.as_raw_fd(),
                    super::FcntlCmd::GetFl,
                    0,
                )? {
                    BsdResult::Ok(flags, ()) => flags,
                    BsdResult::Err(errno) => {
                        return ResultCode::new_err(nx::result::pack_value(
                            rc::RESULT_MODULE,
                            1000 + errno.cast_unsigned(),
                        ));
                    }
                };

                let flags = if nonblocking {
                    current_flags | O_NONBLOCK
                } else {
                    current_flags & !O_NONBLOCK
                };

                if let BsdResult::Err(errno) = socket_server.get_service().fcntl(
                    self.as_raw_fd(),
                    super::FcntlCmd::SetFl,
                    flags,
                )? {
                    return ResultCode::new_err(nx::result::pack_value(
                        rc::RESULT_MODULE,
                        1000 + errno.cast_unsigned(),
                    ));
                }

                Ok(())
            }

            /// Returns the read timeout of this socket.
            /// If the timeout is [`None`], then [`SocketCommon::recv`] calls will block indefinitely.
            fn recv_timeout(&self) -> Result<Option<Duration>> {
                let socket_server_handle = BSD_SERVICE.read();
                let socket_server = socket_server_handle.as_ref().unwrap();

                let mut timeout: BsdDuration = Default::default();
                match socket_server.get_service().get_sock_opt(
                    self.as_raw_fd(),
                    SOL_SOCKET,
                    SocketOptions::ReceiveTimeout as _,
                    Buffer::from_other_mut_var(&mut timeout),
                )? {
                    BsdResult::Ok(_, written_len) => {
                        debug_assert_eq!(written_len as usize, size_of::<BsdDuration>())
                    }
                    BsdResult::Err(errno) => {
                        return ResultCode::new_err(nx::result::pack_value(
                            rc::RESULT_MODULE,
                            1000 + errno.cast_unsigned(),
                        ));
                    }
                }

                if timeout == Default::default() {
                    Ok(None)
                } else {
                    Ok(Some(timeout.into()))
                }
            }

            /// Sets the read timeout to the timeout specified.
            ///
            /// If the value specified is [`None`], then [`SocketCommon::recv`] calls will block
            /// indefinitely. An [`Err`] is returned if the zero [`Duration`] is
            /// passed to this method.
            fn set_read_timeout(&self, timeout: Option<Duration>) -> Result<()> {
                result_return_if!(timeout == Some(Duration::ZERO), rc::ResultInvalidTimeout);

                let socket_server_handle = BSD_SERVICE.read();
                let socket_server = socket_server_handle.as_ref().unwrap();

                let timeout: BsdDuration = timeout.into();
                if let BsdResult::Err(errno) = socket_server.get_service().set_sock_opt(
                    self.as_raw_fd(),
                    SOL_SOCKET,
                    SocketOptions::ReceiveTimeout as _,
                    Buffer::from_other_var(&timeout),
                )? {
                    return ResultCode::new_err(nx::result::pack_value(
                        rc::RESULT_MODULE,
                        1000 + errno.cast_unsigned(),
                    ));
                }

                Ok(())
            }

            /// Returns the write timeout of this socket.
            ///
            ///  If the timeout is [`None`], then [`SocketCommon::send`] calls will block indefinitely.
            fn send_timeout(&self) -> Result<Option<Duration>> {
                let socket_server_handle = BSD_SERVICE.read();
                let socket_server = socket_server_handle.as_ref().unwrap();

                let mut timeout: BsdDuration = Default::default();
                match socket_server.get_service().get_sock_opt(
                    self.as_raw_fd(),
                    SOL_SOCKET,
                    SocketOptions::SendTimeout as _,
                    Buffer::from_other_mut_var(&mut timeout),
                )? {
                    BsdResult::Ok(_, written_len) => {
                        debug_assert_eq!(written_len as usize, size_of::<BsdDuration>())
                    }
                    BsdResult::Err(errno) => {
                        return ResultCode::new_err(nx::result::pack_value(
                            rc::RESULT_MODULE,
                            1000 + errno.cast_unsigned(),
                        ));
                    }
                }

                if timeout == Default::default() {
                    Ok(None)
                } else {
                    Ok(Some(timeout.into()))
                }
            }

            /// Sets the write timeout to the timeout specified.
            ///
            ///  If the value specified is [`None`], then [`write`] calls will block
            /// indefinitely. An [`Err`] is returned if the zero [`Duration`] is
            /// passed to this method.
            fn set_write_timeout(&self, timeout: Option<Duration>) -> Result<()> {
                result_return_if!(timeout == Some(Duration::ZERO), rc::ResultInvalidTimeout);

                let socket_server_handle = BSD_SERVICE.read();
                let socket_server = socket_server_handle.as_ref().unwrap();

                let timeout: BsdDuration = timeout.into();
                if let BsdResult::Err(errno) = socket_server.get_service().set_sock_opt(
                    self.as_raw_fd(),
                    SOL_SOCKET,
                    SocketOptions::SendTimeout as _,
                    Buffer::from_other_var(&timeout),
                )? {
                    return ResultCode::new_err(nx::result::pack_value(
                        rc::RESULT_MODULE,
                        1000 + errno.cast_unsigned(),
                    ));
                }

                Ok(())
            }

            /// Gets the value of the `SO_ERROR` option on this socket.
            ///
            /// This will retrieve the stored error in the underlying socket, clearing
            /// the field in the process. This can be useful for checking errors between
            /// calls.
            fn take_error(&self) -> Result<Option<i32>> {
                let socket_server_handle = BSD_SERVICE.read();

                let socket_server = socket_server_handle.as_ref().unwrap();

                let mut ret_errno: i32 = 0;
                if let BsdResult::Err(errno) = socket_server.get_service().get_sock_opt(
                    self.as_raw_fd(),
                    SOL_SOCKET,
                    SocketOptions::Error as i32,
                    OutAutoSelectBuffer::from_other_mut_var(&mut ret_errno),
                )? {
                    return ResultCode::new_err(nx::result::pack_value(
                        rc::RESULT_MODULE,
                        1000 + errno.cast_unsigned(),
                    ));
                }

                Ok(if ret_errno != 0 {
                    Some(ret_errno)
                } else {
                    None
                })
            }
        }
    }

    use traits::{Pollable, SocketCommon};
    /// Takes a slice of pollable values and requested events returns an iterator over the matched index in the input list and the returned events.
    #[inline(always)]
    pub fn poll<P: traits::Pollable>(
        pollers: &[(P, PollFlags)],
        timeout: Option<i32>,
    ) -> Result<impl Iterator<Item = (usize, PollFlags)>> {
        poll_impl(
            pollers
                .iter()
                .map(|(poll, flags)| PollFd {
                    fd: poll.get_poll_fd(),
                    events: *flags,
                    revents: Default::default(),
                })
                .collect(),
            timeout.unwrap_or(-1),
        )
    }

    #[doc(hidden)]
    fn poll_impl(
        mut fds: Vec<PollFd>,
        timeout: i32,
    ) -> Result<impl Iterator<Item = (usize, PollFlags)>> {
        let socket_server_handle = BSD_SERVICE.read();

        let socket_server = socket_server_handle
            .as_ref()
            .ok_or(rc::ResultNotInitialized::make())?;

        if let BsdResult::Err(errno) = socket_server
            .get_service()
            .poll(Buffer::from_mut_array(fds.as_mut_slice()), timeout)?
        {
            return ResultCode::new_err(nx::result::pack_value(
                rc::RESULT_MODULE,
                1000 + errno.cast_unsigned(),
            ));
        }

        Ok(fds.into_iter().enumerate().filter_map(|(index, pollfd)| {
            if pollfd.events.intersects(pollfd.revents) {
                Some((index, pollfd.revents))
            } else {
                None
            }
        }))
    }

    pub struct TcpListener(i32);

    impl TcpListener {
        pub fn bind(ip: Ipv4Addr, port: u16) -> Result<Self> {
            let socket_server_handle = BSD_SERVICE.read();

            let socket_server = socket_server_handle
                .as_ref()
                .ok_or(rc::ResultNotInitialized::make())?;

            let ipaddr = SocketAddrRepr::from((ip, port));
            let listenfd = match socket_server.get_service().socket(
                super::SocketDomain::INet,
                super::SocketType::Stream,
                super::IpProto::IP,
            )? {
                BsdResult::Ok(ret, ()) => ret,
                BsdResult::Err(errno) => {
                    return ResultCode::new_err(nx::result::pack_value(
                        rc::RESULT_MODULE,
                        1000 + errno.cast_unsigned(),
                    ));
                }
            };

            let yes = 1i32;
            if let BsdResult::Err(errno) = socket_server.get_service().set_sock_opt(
                listenfd,
                SOL_SOCKET,
                SocketOptions::ReuseAddr as i32,
                Buffer::from_other_var(&yes),
            )? {
                return ResultCode::new_err(nx::result::pack_value(
                    rc::RESULT_MODULE,
                    1000 + errno.cast_unsigned(),
                ));
            }

            if let BsdResult::Err(errno) = socket_server
                .get_service()
                .bind(listenfd, Buffer::from_var(&ipaddr))?
            {
                return ResultCode::new_err(nx::result::pack_value(
                    rc::RESULT_MODULE,
                    1000 + errno.cast_unsigned(),
                ));
            };

            if let BsdResult::Err(errno) = socket_server.get_service().listen(listenfd, 5)? {
                return ResultCode::new_err(nx::result::pack_value(
                    rc::RESULT_MODULE,
                    1000 + errno.cast_unsigned(),
                ));
            };

            Ok(Self(listenfd))
        }

        pub fn accept(&self) -> Result<(TcpStream, SocketAddrRepr)> {
            let socket_server_handle = BSD_SERVICE.read();

            let socket_server = socket_server_handle.as_ref().unwrap();

            let mut out_ip: SocketAddrRepr = Default::default();

            match socket_server
                .get_service()
                .accept(self.get_poll_fd(), Buffer::from_mut_var(&mut out_ip))?
            {
                BsdResult::Ok(new_sock, written_sockaddr_size) => {
                    debug_assert!(
                        written_sockaddr_size as usize >= offset_of!(SocketAddrRepr, _zero),
                        "Invalid write length for returned socket addr"
                    );
                    Ok((TcpStream(new_sock), out_ip))
                }
                BsdResult::Err(errno) => ResultCode::new_err(nx::result::pack_value(
                    rc::RESULT_MODULE,
                    1000 + errno.cast_unsigned(),
                )),
            }
        }

        pub fn local_addr(&self) -> Result<SocketAddrRepr> {
            let socket_server_handle = BSD_SERVICE.read();

            let socket_server = socket_server_handle.as_ref().unwrap();

            let mut out_ip: SocketAddrRepr = Default::default();
            match socket_server
                .get_service()
                .get_socket_name(self.0, Buffer::from_mut_var(&mut out_ip))?
            {
                BsdResult::Ok(_, written_sockaddr_size) => {
                    debug_assert!(
                        written_sockaddr_size as usize >= offset_of!(SocketAddrRepr, _zero),
                        "Invalid write length for returned socket addr"
                    );
                    Ok(out_ip)
                }
                BsdResult::Err(errno) => ResultCode::new_err(nx::result::pack_value(
                    rc::RESULT_MODULE,
                    1000 + errno.cast_unsigned(),
                )),
            }
        }

        /// Moves this TCP listener into or out of nonblocking mode.
        ///
        /// This will result in the `accept` operation
        /// becoming nonblocking, i.e., immediately returning from their calls.
        /// If the IO operation is successful, `Ok` is returned and no further
        /// action is required. If the IO operation could not be completed and needs
        /// to be retried, an error with the value set to `EAGAIN` is
        /// returned.
        ///
        /// The function returns a bool representing if the listener was already in non-blocking mode.
        pub fn set_nonblocking(&self, nonblocking: bool) -> Result<bool> {
            const O_NONBLOCK: i32 = 0x800;

            let socket_server_handle = BSD_SERVICE.read();
            let socket_server = socket_server_handle.as_ref().unwrap();

            let current_flags =
                match socket_server
                    .get_service()
                    .fcntl(self.0, super::FcntlCmd::GetFl, 0)?
                {
                    BsdResult::Ok(flags, ()) => flags,
                    BsdResult::Err(errno) => {
                        return ResultCode::new_err(nx::result::pack_value(
                            rc::RESULT_MODULE,
                            1000 + errno.cast_unsigned(),
                        ));
                    }
                };

            let flags = if nonblocking {
                current_flags | O_NONBLOCK
            } else {
                current_flags & !O_NONBLOCK
            };

            if let BsdResult::Err(errno) =
                socket_server
                    .get_service()
                    .fcntl(self.0, super::FcntlCmd::SetFl, flags)?
            {
                return ResultCode::new_err(nx::result::pack_value(
                    rc::RESULT_MODULE,
                    1000 + errno.cast_unsigned(),
                ));
            }

            Ok(current_flags & O_NONBLOCK != 0)
        }
    }

    impl traits::Pollable for TcpListener {
        fn get_poll_fd(&self) -> i32 {
            self.0
        }
    }

    impl Drop for TcpListener {
        fn drop(&mut self) {
            let socket_server_handle = BSD_SERVICE.read();

            let socket_server = socket_server_handle.as_ref().unwrap();
            let _ = socket_server.get_service().close(self.0);
        }
    }
    pub struct TcpStream(i32);

    impl TcpStream {
        fn connect_impl(destination: SocketAddrRepr) -> Result<Self> {
            let socket_server_handle = BSD_SERVICE.read();

            let socket_server = socket_server_handle
                .as_ref()
                .ok_or(rc::ResultNotInitialized::make())?;

            let socket = match socket_server.get_service().socket(
                super::SocketDomain::INet,
                super::SocketType::Stream,
                super::IpProto::IP,
            )? {
                BsdResult::Ok(ret, ()) => ret,
                BsdResult::Err(errno) => {
                    return ResultCode::new_err(nx::result::pack_value(
                        rc::RESULT_MODULE,
                        1000 + errno.cast_unsigned(),
                    ));
                }
            };

            if let BsdResult::Err(errno) = socket_server
                .get_service()
                .connect(socket, Buffer::from_var(&destination))?
            {
                return ResultCode::new_err(nx::result::pack_value(
                    rc::RESULT_MODULE,
                    1000 + errno.cast_unsigned(),
                ));
            };

            Ok(Self(socket))
        }

        pub fn linger(&self) -> Result<Option<Duration>> {
            let socket_server_handle = BSD_SERVICE.read();

            let socket_server = socket_server_handle.as_ref().unwrap();

            let mut linger: Linger = Default::default();
            if let BsdResult::Err(errno) = socket_server.get_service().get_sock_opt(
                self.0,
                SOL_SOCKET,
                SocketOptions::Linger as i32,
                OutAutoSelectBuffer::from_other_mut_var(&mut linger),
            )? {
                return ResultCode::new_err(nx::result::pack_value(
                    rc::RESULT_MODULE,
                    1000 + errno.cast_unsigned(),
                ));
            }

            Ok(linger.into())
        }
        /// Gets the value of the TCP_NODELAY option on this socket.
        ///
        /// For more information about this option, see `TcpStream::set_nodelay`.
        pub fn nodelay(&self) -> Result<bool> {
            let socket_server_handle = BSD_SERVICE.read();

            let socket_server = socket_server_handle.as_ref().unwrap();

            let mut delay: i32 = 0;
            match socket_server.get_service().get_sock_opt(
                self.0,
                IpProto::IP as _,
                TcpOptions::NoDelay as _,
                Buffer::from_other_mut_var(&mut delay),
            )? {
                BsdResult::Ok(_, written_data_len) => {
                    debug_assert_ne!(written_data_len, 0); // we're reading an i32, but we only care if it's zero or not so any sized write is valid.
                    Ok(delay != 0)
                }
                BsdResult::Err(errno) => ResultCode::new_err(nx::result::pack_value(
                    rc::RESULT_MODULE,
                    1000 + errno.cast_unsigned(),
                )),
            }
        }

        /// Sets the value of the TCP_NODELAY option on this socket.
        ///
        /// If set, this option disables the Nagle algorithm.
        /// This means that segments are always sent as soon as possible, even
        /// if there is only a small amount of data. When not set, data is
        /// buffered until there is a sufficient amount to send out, thereby
        /// avoiding the frequent sending of small packets.
        pub fn set_nodelay(&self, value: bool) -> Result<()> {
            let socket_server_handle = BSD_SERVICE.read();
            let socket_server = socket_server_handle.as_ref().unwrap();

            if let BsdResult::Err(errno) = socket_server.get_service().set_sock_opt(
                self.0,
                SOL_SOCKET,
                SocketOptions::Broadcast as _,
                Buffer::from_other_var(&(value as i32)),
            )? {
                return ResultCode::new_err(nx::result::pack_value(
                    rc::RESULT_MODULE,
                    1000 + errno.cast_unsigned(),
                ));
            }

            Ok(())
        }

        /// Shuts down the read, write, or both halves of this connection.
        ///
        /// This function will cause all pending and future I/O on the specified
        /// portions to return immediately with an appropriate value (see the
        /// documentation of [`ShutdownMode`]).
        ///
        /// # Examples
        ///
        /// ```no_run
        /// use nx::socket::net::{Shutdown, TcpStream};
        ///
        /// let stream = TcpStream::connect(Ipv4Addr::LOCALHOST, Some(8080))
        ///                        .expect("Couldn't connect to the server...");
        /// stream.shutdown(Shutdown::Both).expect("shutdown call failed");
        /// ```
        pub fn shutdown(&self, mode: ShutdownMode) -> Result<()> {
            let socket_server_handle = BSD_SERVICE.read();
            let socket_server = socket_server_handle.as_ref().unwrap();

            if let BsdResult::Err(errno) = socket_server.get_service().shutdown(self.0, mode)? {
                return ResultCode::new_err(nx::result::pack_value(
                    rc::RESULT_MODULE,
                    1000 + errno.cast_unsigned(),
                ));
            }

            Ok(())
        }
    }

    impl Drop for TcpStream {
        fn drop(&mut self) {
            let socket_server_handle = BSD_SERVICE.read();

            let socket_server = socket_server_handle.as_ref().unwrap();
            let _ = socket_server.get_service().close(self.0);
        }
    }

    unsafe impl traits::SocketCommon for TcpStream {
        #[inline(always)]
        fn as_raw_fd(&self) -> i32 {
            self.0
        }

        #[inline(always)]
        fn connect<A: Into<Ipv4Addr>>(destination: A, port: u16) -> Result<Self> {
            let destination = (destination.into(), port).into();
            Self::connect_impl(destination)
        }

        #[inline(always)]
        fn try_clone(&self) -> Result<Self> {
            Ok(Self(self.0))
        }
    }

    /// A UDP socket.
    ///
    /// After creating a `UdpSocket` by [`bind`]ing it to a socket address, data can be
    /// [sent to] and [received from] any other socket address.
    ///
    /// Although UDP is a connectionless protocol, this implementation provides an interface
    /// to set an address where data should be sent and received from. After setting a remote
    /// address with [`connect`], data can be sent to and received from that address with
    /// [`send`] and [`recv`].
    ///
    /// As stated in the User Datagram Protocol's specification in [IETF RFC 768], UDP is
    /// an unordered, unreliable protocol; refer to [`TcpListener`] and [`TcpStream`] for TCP
    /// primitives.
    ///
    /// [`bind`]: UdpSocket::bind
    /// [`connect`]: SocketCommon::connect
    /// [IETF RFC 768]: https://tools.ietf.org/html/rfc768
    /// [`recv`]: SocketCommon::recv
    /// [received from]: UdpSocket::recv_from
    /// [`send`]: SocketCommon::send
    /// [sent to]: UdpSocket::send_to
    /// [`TcpListener`]: TcpListener
    /// [`TcpStream`]: TcpStream
    ///
    /// # Examples
    ///
    /// ```no_run
    ///
    /// let socket = UdpSocket::bind(Ipv4Addr::LOCALHOST, Some(34254))?;
    ///
    /// // Receives a single datagram message on the socket. If `buf` is too small to hold
    /// // the message, it will be cut off.
    /// let mut buf = [0; 10];
    /// let (amt, src_ip, src_port) = socket.recv_from(&mut buf)?;
    ///
    /// // Redeclare `buf` as slice of the received data and send reverse data back to origin.
    /// let buf = &mut buf[..amt];
    /// buf.reverse();
    /// socket.send_to(buf, (src_ip, src_port))?;
    ///
    /// ```
    pub struct UdpSocket(i32);

    impl UdpSocket {
        /// Creates a UDP socket from the given address.
        ///
        /// The address type can be any implementor of [`Into<Ipv4Addr>`].
        ///
        /// # Examples
        ///
        /// Creates a UDP socket bound to `127.0.0.1:3400`:
        ///
        /// ```no_run
        /// use std::net::UdpSocket;
        ///
        /// let socket = UdpSocket::bind(Ipv4Addr::LOCALHOST,Some(3400)).expect("couldn't bind to address");
        /// ```
        ///
        /// Creates a UDP socket bound to a port assigned by the operating system
        /// at `127.0.0.1`.
        ///
        /// ```no_run
        /// use std::net::UdpSocket;
        ///
        /// let socket = UdpSocket::bind(Ipv4Addr::LOCALHOST, None).unwrap();
        /// ```
        ///
        /// Note that `bind` declares the scope of your network connection.
        /// You can only receive datagrams from and send datagrams to
        /// participants in that view of the network.
        /// For instance, binding to a loopback address as in the example
        /// above will prevent you from sending datagrams to another device
        /// in your local network.
        ///
        /// In order to limit your view of the network the least, `bind` to
        /// [`Ipv4Addr::UNSPECIFIED`].
        pub fn bind<A: Into<Ipv4Addr>>(addr: A, port: Option<u16>) -> Result<Self> {
            let socket_server_handle = BSD_SERVICE.read();

            let socket_server = socket_server_handle
                .as_ref()
                .ok_or(rc::ResultNotInitialized::make())?;
            let socket = match socket_server.get_service().socket(
                super::SocketDomain::INet,
                super::SocketType::DataGram,
                super::IpProto::UDP,
            )? {
                BsdResult::Ok(ret, ()) => ret,
                BsdResult::Err(errno) => {
                    return ResultCode::new_err(nx::result::pack_value(
                        rc::RESULT_MODULE,
                        1000 + errno.cast_unsigned(),
                    ));
                }
            };

            let ipaddr: Ipv4Addr = addr.into();
            let socketaddr = SocketAddrRepr::from((ipaddr, port.unwrap_or(0)));
            if let BsdResult::Err(errno) = socket_server
                .get_service()
                .bind(socket, Buffer::from_var(&socketaddr))?
            {
                return ResultCode::new_err(nx::result::pack_value(
                    rc::RESULT_MODULE,
                    1000 + errno.cast_unsigned(),
                ));
            };
            Err(rc::ResultNotInitialized::make())
        }

        fn connect_impl(destination: SocketAddrRepr) -> Result<Self> {
            let socket_server_handle = BSD_SERVICE.read();

            let socket_server = socket_server_handle
                .as_ref()
                .ok_or(rc::ResultNotInitialized::make())?;

            let socket = match socket_server.get_service().socket(
                super::SocketDomain::INet,
                super::SocketType::DataGram,
                super::IpProto::UDP,
            )? {
                BsdResult::Ok(ret, ()) => ret,
                BsdResult::Err(errno) => {
                    return ResultCode::new_err(nx::result::pack_value(
                        rc::RESULT_MODULE,
                        1000 + errno.cast_unsigned(),
                    ));
                }
            };

            if let BsdResult::Err(errno) = socket_server
                .get_service()
                .connect(socket, Buffer::from_var(&destination))?
            {
                return ResultCode::new_err(nx::result::pack_value(
                    rc::RESULT_MODULE,
                    1000 + errno.cast_unsigned(),
                ));
            };

            Ok(Self(socket))
        }

        /// Receives data on the socket from the remote address to which it is connected.
        /// On success, returns the number of bytes read and the origin.
        ///
        ///The function must be called with valid byte array buf of sufficient size to hold the message bytes. If a message is too long to fit in the supplied buffer, excess bytes may be discarded.
        pub fn recv_from(&self, buffer: &mut [u8]) -> Result<(usize, Ipv4Addr, u16)> {
            let socket_server_handle = BSD_SERVICE.read();

            let socket_server = socket_server_handle.as_ref().unwrap();

            let mut out_addr: SocketAddrRepr = Default::default();
            match socket_server.get_service().recv_from(
                self.0,
                ReadFlags::None(),
                Buffer::from_mut_array(buffer),
                Buffer::from_mut_var(&mut out_addr),
            )? {
                BsdResult::Ok(ret, ()) => Ok((
                    ret as usize,
                    Ipv4Addr::from_bits(u32::from_be_bytes(out_addr.addr)),
                    u16::from_be(out_addr.port),
                )),
                BsdResult::Err(errno) => ResultCode::new_err(nx::result::pack_value(
                    rc::RESULT_MODULE,
                    1000 + errno.cast_unsigned(),
                )),
            }
        }
        /// Receives a single datagram message on the socket, without removing it from the queue.
        /// On success, returns the number of bytes read and the origin.
        ///
        /// The function must be called with valid byte array buf of sufficient size to hold the message bytes.
        /// If a message is too long to fit in the supplied buffer, excess bytes may be discarded.
        ///
        /// Successive calls return the same data. This is accomplished by passing `MSG_PEEK` as a flag to the underlying `recvfrom` system call.
        ///
        /// Do not use this function to implement busy waiting, instead use [`poll`][`nx::socket::net::poll`] to synchronize IO events on one or more sockets.
        pub fn peek_from(&self, buffer: &mut [u8]) -> Result<(usize, Ipv4Addr, u16)> {
            let socket_server_handle = BSD_SERVICE.read();

            let socket_server = socket_server_handle.as_ref().unwrap();

            let mut out_addr: SocketAddrRepr = Default::default();
            match socket_server.get_service().recv_from(
                self.0,
                ReadFlags::Peek(),
                Buffer::from_mut_array(buffer),
                Buffer::from_mut_var(&mut out_addr),
            )? {
                BsdResult::Ok(ret, ()) => Ok((
                    ret as usize,
                    Ipv4Addr::from_bits(u32::from_be_bytes(out_addr.addr)),
                    u16::from_be(out_addr.port),
                )),
                BsdResult::Err(errno) => ResultCode::new_err(nx::result::pack_value(
                    rc::RESULT_MODULE,
                    1000 + errno.cast_unsigned(),
                )),
            }
        }

        /// Receives data on the socket from the remote address to which it is connected.
        /// On success, returns the number of bytes read and the origin or a None value if there is no data.
        ///
        ///The function must be called with valid byte array buf of sufficient size to hold the message bytes. If a message is too long to fit in the supplied buffer, excess bytes may be discarded.
        pub fn recv_from_non_blocking(
            &self,
            buffer: &mut [u8],
        ) -> Result<Option<(usize, Ipv4Addr, u16)>> {
            let socket_server_handle = BSD_SERVICE.read();

            let socket_server = socket_server_handle.as_ref().unwrap();

            let mut out_addr: SocketAddrRepr = Default::default();
            match socket_server.get_service().recv_from(self.0, ReadFlags::DontWait(), Buffer::from_mut_array(buffer), Buffer::from_mut_var(&mut out_addr))? {
                BsdResult::Ok(ret, ()) => {
                    Ok(Some((ret as usize, Ipv4Addr::from_bits(u32::from_be_bytes(out_addr.addr)), u16::from_be(out_addr.port))))
                },
                BsdResult::Err(11) /* EAGAIN */ => {
                    Ok(None)
                }
                BsdResult::Err(errno) => {
                    ResultCode::new_err(nx::result::pack_value(
                        rc::RESULT_MODULE,
                        1000 + errno.cast_unsigned(),
                    ))
                }
            }
        }

        /// Sends data on the socket to the remote address provided (no call to `UdpSocket::connect` is necessary).
        /// Unlike `std::net::UdpSocket`, this method does not return length of the written data.
        /// All data is sent or an error is returned.
        #[inline(always)]
        pub fn send_to<A: Into<SocketAddrRepr>>(&self, data: &[u8], destination: A) -> Result<()> {
            self.send_to_impl(data, destination.into())
        }

        fn send_to_impl(&self, data: &[u8], destination: SocketAddrRepr) -> Result<()> {
            let socket_server_handle = BSD_SERVICE.read();

            let socket_server = socket_server_handle.as_ref().unwrap();
            match socket_server.get_service().send_to(
                self.0,
                SendFlags::None(),
                Buffer::from_array(data),
                Buffer::from_var(&destination),
            )? {
                BsdResult::Ok(_, ()) => Ok(()),
                BsdResult::Err(errno) => ResultCode::new_err(nx::result::pack_value(
                    rc::RESULT_MODULE,
                    1000 + errno.cast_unsigned(),
                )),
            }
        }
        /// Gets the value of the SO_BROADCAST option for this socket.
        ///
        /// For more information about this option, see `UdpSocket::set_broadcast``.
        pub fn broadcast(&self) -> Result<bool> {
            let socket_server_handle = BSD_SERVICE.read();

            let socket_server = socket_server_handle.as_ref().unwrap();

            let mut broadcast: i32 = 0;
            match socket_server.get_service().get_sock_opt(
                self.0,
                SOL_SOCKET,
                SocketOptions::Broadcast as _,
                Buffer::from_other_mut_var(&mut broadcast),
            )? {
                BsdResult::Ok(_, written_data_len) => {
                    debug_assert_ne!(written_data_len, 0); // we're reading an i32, but we only care if it's zero or not so any sized write is valid.
                    Ok(broadcast != 0)
                }
                BsdResult::Err(errno) => ResultCode::new_err(nx::result::pack_value(
                    rc::RESULT_MODULE,
                    1000 + errno.cast_unsigned(),
                )),
            }
        }
        /// Sets the value of the SO_BROADCAST option for this socket.
        ///
        /// When enabled, this socket is allowed to send packets to a broadcast address.
        pub fn set_broadcast(&self, value: bool) -> Result<()> {
            let socket_server_handle = BSD_SERVICE.read();
            let socket_server = socket_server_handle.as_ref().unwrap();

            if let BsdResult::Err(errno) = socket_server.get_service().set_sock_opt(
                self.0,
                SOL_SOCKET,
                SocketOptions::Broadcast as _,
                Buffer::from_other_var(&(value as u8)),
            )? {
                return ResultCode::new_err(nx::result::pack_value(
                    rc::RESULT_MODULE,
                    1000 + errno.cast_unsigned(),
                ));
            }

            Ok(())
        }
        /// Gets the value of the IP_MULTICAST_LOOP option for this socket.
        ///
        ///For more information about this option, see `UdpSocket::set_multicast_loop``.
        pub fn multicast_loop(&self) -> Result<bool> {
            let socket_server_handle = BSD_SERVICE.read();

            let socket_server = socket_server_handle.as_ref().unwrap();

            let mut mm_loop: u8 = 0;
            match socket_server.get_service().get_sock_opt(
                self.0,
                IpProto::IP as _,
                IpOptions::MulticastLoopback as _,
                Buffer::from_other_mut_var(&mut mm_loop),
            )? {
                BsdResult::Ok(_, written_data_len) => {
                    debug_assert_ne!(written_data_len, 0);
                    Ok(mm_loop != 0)
                }
                BsdResult::Err(errno) => ResultCode::new_err(nx::result::pack_value(
                    rc::RESULT_MODULE,
                    1000 + errno.cast_unsigned(),
                )),
            }
        }
        /// Sets the value of the IP_MULTICAST_LOOP option for this socket.
        ///
        /// If enabled, multicast packets will be looped back to the local socket.
        pub fn set_multicast_loop(&self, value: bool) -> Result<()> {
            let socket_server_handle = BSD_SERVICE.read();
            let socket_server = socket_server_handle.as_ref().unwrap();

            if let BsdResult::Err(errno) = socket_server.get_service().set_sock_opt(
                self.0,
                IpProto::IP as _,
                IpOptions::MulticastLoopback as _,
                Buffer::from_other_var(&(value as u8)),
            )? {
                return ResultCode::new_err(nx::result::pack_value(
                    rc::RESULT_MODULE,
                    1000 + errno.cast_unsigned(),
                ));
            }

            Ok(())
        }
        /// Gets the value of the IP_MULTICAST_TTL option for this socket.
        ///
        /// For more information about this option, see `UdpSocket::set_multicast_ttl``.
        pub fn multicast_ttl(&self) -> Result<u8> {
            let socket_server_handle = BSD_SERVICE.read();

            let socket_server = socket_server_handle.as_ref().unwrap();

            let mut ttl: u8 = 0;
            match socket_server.get_service().get_sock_opt(
                self.0,
                IpProto::IP as _,
                IpOptions::MulticastTimeToLive as _,
                Buffer::from_other_mut_var(&mut ttl),
            )? {
                BsdResult::Ok(_, written_data_len) => {
                    debug_assert_ne!(written_data_len, 0);
                    Ok(ttl)
                }
                BsdResult::Err(errno) => ResultCode::new_err(nx::result::pack_value(
                    rc::RESULT_MODULE,
                    1000 + errno.cast_unsigned(),
                )),
            }
        }
        /// Sets the value of the IP_MULTICAST_TTL option for this socket.
        ///
        ///Indicates the time-to-live value of outgoing multicast packets for this socket. The default value is 1 which means that multicast packets don’t leave the local network unless explicitly requested.
        pub fn set_multicast_ttl(&self, ttl: u8) -> Result<()> {
            let socket_server_handle = BSD_SERVICE.read();
            let socket_server = socket_server_handle.as_ref().unwrap();

            if let BsdResult::Err(errno) = socket_server.get_service().set_sock_opt(
                self.0,
                IpProto::IP as _,
                IpOptions::MulticastTimeToLive as _,
                Buffer::from_var(&ttl),
            )? {
                return ResultCode::new_err(nx::result::pack_value(
                    rc::RESULT_MODULE,
                    1000 + errno.cast_unsigned(),
                ));
            }

            Ok(())
        }
        /// Executes an operation of the `IP_ADD_MEMBERSHIP` type.
        ///
        /// This function specifies a new multicast group for this socket to join.
        /// The address must be a valid multicast address, and interface is the address of the local interface
        /// with which the system should join the multicast group.
        /// If it’s equal to `Ipv4Addr::UNSPECIFIED` then an appropriate interface is chosen by the system.
        pub fn join_multicast_group(
            &self,
            multicast_addr: Ipv4Addr,
            interface_addr: Ipv4Addr,
        ) -> Result<()> {
            let socket_server_handle = BSD_SERVICE.read();
            let socket_server = socket_server_handle.as_ref().unwrap();

            let ip_request = IpMulticastRequest {
                multicast_addr,
                interface_addr,
            };
            if let BsdResult::Err(errno) = socket_server.get_service().set_sock_opt(
                self.0,
                IpProto::IP as _,
                IpOptions::MulticastAddMembership as _,
                Buffer::from_other_var(&ip_request),
            )? {
                return ResultCode::new_err(nx::result::pack_value(
                    rc::RESULT_MODULE,
                    1000 + errno.cast_unsigned(),
                ));
            }

            Ok(())
        }
        /// Executes an operation of the IP_DROP_MEMBERSHIP type.
        ///
        /// For more information about this option, see `UdpSocket::join_multicast`.
        pub fn leave_multicast_group(
            &self,
            multicast_addr: Ipv4Addr,
            interface_addr: Ipv4Addr,
        ) -> Result<()> {
            let socket_server_handle = BSD_SERVICE.read();
            let socket_server = socket_server_handle.as_ref().unwrap();

            let ip_request = IpMulticastRequest {
                multicast_addr,
                interface_addr,
            };
            if let BsdResult::Err(errno) = socket_server.get_service().set_sock_opt(
                self.0,
                IpProto::IP as _,
                IpOptions::MulticastDropMembership as _,
                Buffer::from_other_var(&ip_request),
            )? {
                return ResultCode::new_err(nx::result::pack_value(
                    rc::RESULT_MODULE,
                    1000 + errno.cast_unsigned(),
                ));
            }

            Ok(())
        }
    }

    impl Drop for UdpSocket {
        fn drop(&mut self) {
            let socket_server_handle = BSD_SERVICE.read();

            let socket_server = socket_server_handle.as_ref().unwrap();
            let _ = socket_server.get_service().close(self.0);
        }
    }

    unsafe impl traits::SocketCommon for UdpSocket {
        #[inline(always)]
        fn as_raw_fd(&self) -> i32 {
            self.0
        }

        #[inline(always)]
        fn connect<A: Into<Ipv4Addr>>(destination: A, port: u16) -> Result<Self> {
            let destination = (destination.into(), port).into();
            Self::connect_impl(destination)
        }

        #[inline(always)]
        fn try_clone(&self) -> Result<Self> {
            Ok(Self(self.0))
        }
    }

    impl core::fmt::Write for TcpStream {
        fn write_str(&mut self, s: &str) -> core::fmt::Result {
            match self.send(s.as_bytes()) {
                Ok(_) => Ok(()),
                Err(_) => Err(core::fmt::Error),
            }
        }
    }

    impl core::fmt::Write for UdpSocket {
        fn write_str(&mut self, s: &str) -> core::fmt::Result {
            match self.send(s.as_bytes()) {
                Ok(_) => Ok(()),
                Err(_) => Err(core::fmt::Error),
            }
        }
    }
    pub mod io_impls {
        use crate::{result::ResultCode, socket::net::traits::SocketCommon};

        use super::{TcpStream, UdpSocket};

        impl embedded_io::ErrorType for TcpStream {
            type Error = ResultCode;
        }
        impl embedded_io::ErrorType for UdpSocket {
            type Error = ResultCode;
        }

        impl embedded_io::Read for TcpStream {
            fn read(&mut self, buf: &mut [u8]) -> Result<usize, Self::Error> {
                if buf.len() == 0 {
                    return Ok(0);
                }

                self.recv(buf).map(|l| l as usize)
            }
        }

        impl embedded_io::Read for UdpSocket {
            fn read(&mut self, buf: &mut [u8]) -> Result<usize, Self::Error> {
                if buf.len() == 0 {
                    return Ok(0);
                }

                self.recv(buf).map(|l| l as usize)
            }
        }

        impl embedded_io::Write for TcpStream {
            fn write(&mut self, buf: &[u8]) -> core::result::Result<usize, Self::Error> {
                if buf.len() == 0 {
                    return Ok(0);
                }

                match self.send(buf) {
                    Ok(0) => Err(ResultCode::new(1005)),
                    Ok(l) => Ok(l as usize),
                    Err(e) => Err(e),
                }
            }

            fn flush(&mut self) -> core::result::Result<(), Self::Error> {
                Ok(())
            }
        }

        impl embedded_io::Write for UdpSocket {
            fn write(&mut self, buf: &[u8]) -> core::result::Result<usize, Self::Error> {
                if buf.len() == 0 {
                    return Ok(0);
                }

                match self.send(buf) {
                    Ok(0) => Err(ResultCode::new(1005)),
                    Ok(l) => Ok(l as usize),
                    Err(e) => Err(e),
                }
            }

            fn flush(&mut self) -> core::result::Result<(), Self::Error> {
                Ok(())
            }
        }
    }
}

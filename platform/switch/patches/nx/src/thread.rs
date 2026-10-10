//! Threading support and wrappers

use ::alloc::boxed::Box;
use ::alloc::string::String;
use ::alloc::string::ToString;
use ::alloc::sync::Arc;
use imp::LibNxThreadVars;

use crate::diag::abort;
use crate::diag::abort::AbortLevel;
use crate::result::*;
use crate::svc;
use crate::util;
use core::cell::UnsafeCell;
use core::fmt;
use core::marker::PhantomData;
use core::mem;
use core::mem::ManuallyDrop;
use core::pin::Pin;
pub mod rc;
//pub mod _local;
pub mod scoped;

type ThreadId = u64;

/// Thread factory, which can be used in order to configure the properties of
/// a new thread.
///
/// Methods can be chained on it in order to configure it.
///
/// The two configurations available are:
///
/// - [`name`]: specifies an [associated name for the thread][naming-threads]
/// - [`stack_size`]: specifies the [desired stack size for the thread][stack-size]
///
/// The [`spawn`] method will take ownership of the builder and create an
/// [`crate::result::Result`] to the thread handle with the given configuration.
///
/// The [`thread::spawn`] free function uses a `Builder` with default
/// configuration and [`unwrap`]s its return value.
///
/// You may want to use [`spawn`] instead of [`thread::spawn`], when you want
/// to recover from a failure to launch a thread, indeed the free function will
/// panic where the `Builder` method will return a [`crate::result::Result`].
///
/// # Examples
///
/// ```
/// use nx::thread;
///
/// let builder = thread::Builder::new();
///
/// let handler = builder.spawn(|| {
///     // thread code
/// }).unwrap();
///
/// handler.join().unwrap();
/// ```
///
/// [`stack_size`]: Builder::stack_size
/// [`name`]: Builder::name
/// [`spawn`]: Builder::spawn
/// [`thread::spawn`]: spawn
/// [`Result`]: crate::result::Result
/// [`unwrap`]: crate::result::Result::unwrap
/// [naming-threads]: ./index.html#naming-threads
/// [stack-size]: ./index.html#stack-size
#[derive(Default)]
#[must_use = "must eventually spawn the thread"]
pub struct Builder {
    // A name for the thread-to-be, for identification in panic messages
    name: Option<String>,
    // The size of the stack for the spawned thread in bytes
    stack_size: Option<usize>,
    // The priority of the thread to spawn
    priority: Option<ThreadPriority>,
    // The requested core of the thread to spawn
    core: Option<ThreadStartCore>,
}

impl Builder {
    /// Generates the base configuration for spawning a thread, from which
    /// configuration methods can be chained.
    ///
    /// # Examples
    ///
    /// ```
    /// use nx::thread;
    ///
    /// let builder = thread::Builder::new()
    ///                               .name("foo".into())
    ///                               .stack_size(32 * 1024);
    ///
    /// let handler = builder.spawn(|| {
    ///     // thread code
    /// }).unwrap();
    ///
    /// handler.join().unwrap();
    /// ```
    pub fn new() -> Builder {
        Default::default()
    }

    /// Names the thread-to-be. Currently the name is used for identification
    /// only in panic messages.
    ///
    /// The name must not contain null bytes (`\0`).
    ///
    /// For more information about named threads, see
    /// [this module-level documentation][naming-threads].
    ///
    /// # Examples
    ///
    /// ```
    /// use nx::thread;
    ///
    /// let builder = thread::Builder::new()
    ///     .name("foo".into());
    ///
    /// let handler = builder.spawn(|| {
    ///     assert_eq!(thread::current().name(), Some("foo"))
    /// }).unwrap();
    ///
    /// handler.join().unwrap();
    /// ```
    ///
    /// [naming-threads]: ./index.html#naming-threads
    pub fn name<S: ToString>(mut self, name: S) -> Builder {
        self.name = Some(name.to_string());
        self
    }

    /// Sets the priority for the new thread
    ///
    /// # Examples
    ///
    /// ```
    /// use nx::thread;
    ///
    /// let builder = thread::Builder::new().priority(ThreadPriority::Default);
    /// ```
    pub fn priority(mut self, priority: ThreadPriority) -> Builder {
        self.priority = Some(priority);
        self
    }

    /// Sets the size of the stack (in bytes) for the new thread, to be allocated during stack creation
    ///
    /// The actual stack size may be greater to align up to the page size.
    ///
    /// For more information about the stack size for threads, see
    /// [this module-level documentation][stack-size].
    ///
    /// # Examples
    ///
    /// ```
    /// use nx::thread;
    ///
    /// let builder = thread::Builder::new().stack_size(32 * 1024);
    /// ```
    pub fn stack_size(mut self, size: usize) -> Builder {
        self.stack_size = Some(size);
        self
    }

    /// Sets the CPU core for the new thread to start on
    ///
    /// # Examples
    ///
    /// ```
    /// use nx::thread;
    ///
    /// let builder = thread::Builder::new().core(ThreadStartCore::Default);
    /// ```
    pub fn core(mut self, core: ThreadStartCore) -> Builder {
        self.core = Some(core);
        self
    }

    /// Spawns a new thread by taking ownership of the `Builder`, and returns an
    /// [`crate::result::Result`] to its [`JoinHandle`].
    ///
    /// The spawned thread may outlive the caller (unless the caller thread
    /// is the main thread; the whole process is terminated when the main
    /// thread finishes). The join handle can be used to block on
    /// termination of the spawned thread, including recovering its panics.
    ///
    /// For a more complete documentation see [`thread::spawn`][`spawn`].
    ///
    /// # Errors
    ///
    /// Unlike the [`spawn`] free function, this method yields an
    /// [`crate::result::Result`] to capture any failure to create the thread at
    /// the OS level.
    ///
    /// [`crate::result::Result`]: crate::result::Result
    ///
    /// # Panics
    ///
    /// Panics if a thread name was set and it contained null bytes.
    ///
    /// # Examples
    ///
    /// ```
    /// use nx::thread;
    ///
    /// let builder = thread::Builder::new();
    ///
    /// let handler = builder.spawn(|| {
    ///     // thread code
    /// }).unwrap();
    ///
    /// handler.join().unwrap();
    /// ```
    pub fn spawn<F, T>(self, f: F) -> crate::result::Result<JoinHandle<T, false>>
    where
        F: FnOnce() -> T,
        F: Send + 'static,
        T: Send + 'static,
    {
        unsafe { self.spawn_unchecked(f) }
    }

    pub fn spawn_blocking<F, T>(self, f: F) -> crate::result::Result<JoinHandle<T, true>>
    where
        F: FnOnce() -> T,
        F: Send + 'static,
        T: Send + 'static,
    {
        unsafe { self.spawn_unchecked(f) }
    }

    /// Spawns a new thread without any lifetime restrictions by taking ownership
    /// of the `Builder`, and returns an [`crate::result::Result`] to its [`JoinHandle`].
    ///
    /// The spawned thread may outlive the caller (unless the caller thread
    /// is the main thread; the whole process is terminated when the main
    /// thread finishes). The join handle can be used to block on
    /// termination of the spawned thread, including recovering its panics.
    ///
    /// This method is identical to [`thread::Builder::spawn`][`Builder::spawn`],
    /// except for the relaxed lifetime bounds, which render it unsafe.
    /// For a more complete documentation see [`thread::spawn`][`spawn`].
    ///
    /// # Errors
    ///
    /// Unlike the [`spawn`] free function, this method yields an
    /// [`crate::result::Result`] to capture any failure to create the thread at
    /// the OS level.
    ///
    /// # Panics
    ///
    /// Panics if a thread name was set and it contained null bytes.
    ///
    /// # Safety
    ///
    /// The caller has to ensure that the spawned thread does not outlive any
    /// references in the supplied thread closure and its return type.
    /// This can be guaranteed in two ways:
    ///
    /// - ensure that [`join`][`JoinHandle::join`] is called before any referenced
    ///   data is dropped
    /// - use only types with `'static` lifetime bounds, i.e., those with no or only
    ///   `'static` references (both [`thread::Builder::spawn`][`Builder::spawn`]
    ///   and [`thread::spawn`][`spawn`] enforce this property statically)
    ///
    /// # Examples
    ///
    /// ```
    /// use nx::thread;
    ///
    /// let builder = thread::Builder::new();
    ///
    /// let x = 1;
    /// let thread_x = &x;
    ///
    /// let handler = unsafe {
    ///     builder.spawn_unchecked(move || {
    ///         println!("x = {}", *thread_x);
    ///     }).unwrap()
    /// };
    ///
    /// // caller has to ensure `join()` is called, otherwise
    /// // it is possible to access freed memory if `x` gets
    /// // dropped before the thread closure is executed!
    /// handler.join().unwrap();
    /// ```
    ///
    /// [`Result`]: crate::result::Result
    pub unsafe fn spawn_unchecked<F, T, const WAIT_ON_DROP: bool>(
        self,
        f: F,
    ) -> crate::result::Result<JoinHandle<T, WAIT_ON_DROP>>
    where
        F: FnOnce() -> T + 'static,
        F: Send,
        T: Send + 'static,
    {
        Ok(JoinHandle(unsafe { self.spawn_unchecked_(f, None) }?))
    }

    #[allow(unsafe_op_in_unsafe_fn)]
    unsafe fn spawn_unchecked_<F, T>(
        self,
        f: F,
        scope_data: Option<Arc<scoped::ScopeData>>,
    ) -> crate::result::Result<JoinInner<'static, T>>
    where
        F: FnOnce() -> T + 'static,
        F: Send,
        T: Send + 'static,
    {
        let Builder {
            name,
            stack_size,
            priority,
            core,
        } = self;

        let stack_size = stack_size.unwrap_or(0x8000);
        let name = name
            .map(|s| ThreadName::from_string(&s))
            .unwrap_or(ThreadName::new());
        let priority = priority.unwrap_or_default();
        let core = core.unwrap_or_default();

        let my_thread = Thread::new_inner(name);
        let _their_thread = my_thread.clone();

        let my_packet: Arc<Packet<'static, T>> = Arc::new(Packet {
            scope: scope_data,
            result: UnsafeCell::new(None),
            _marker: PhantomData,
        });
        let their_packet = my_packet.clone();

        // Pass `f` in `MaybeUninit` because actually that closure might *run longer than the lifetime of `F`*.
        // See <https://github.com/rust-lang/rust/issues/101983> for more details.
        // To prevent leaks we use a wrapper that drops its contents.
        #[repr(transparent)]
        struct MaybeDangling<T>(mem::MaybeUninit<T>);
        impl<T> MaybeDangling<T> {
            fn new(x: T) -> Self {
                MaybeDangling(mem::MaybeUninit::new(x))
            }
            fn into_inner(self) -> T {
                // Make sure we don't drop.
                let this = ManuallyDrop::new(self);
                // SAFETY: we are always initialized.
                unsafe { this.0.assume_init_read() }
            }
        }
        impl<T> Drop for MaybeDangling<T> {
            fn drop(&mut self) {
                // SAFETY: we are always initialized.
                unsafe { self.0.assume_init_drop() };
            }
        }

        let f = MaybeDangling::new(f);
        let main = move || {
            let f = f.into_inner();
            let try_result = unwinding::panic::catch_unwind(core::panic::AssertUnwindSafe(f));
            // SAFETY: `their_packet` as been built just above and moved by the
            // closure (it is an Arc<...>) and `my_packet` will be stored in the
            // same `JoinInner` as this closure meaning the mutation will be
            // safe (not modify it and affect a value far away).
            unsafe { *their_packet.result.get() = Some(try_result) };
            // Here `their_packet` gets dropped, and if this is the last `Arc` for that packet that
            // will call `decrement_num_running_threads` and therefore signal that this thread is
            // done.
            drop(their_packet);
            // Here, the lifetime `'static` can end. `main` keeps running for a bit
            // after that before returning itself.
        };

        if let Some(scope_data) = &my_packet.scope {
            scope_data.increment_num_running_threads();
        }

        let main = Box::new(main);
        // SAFETY: dynamic size and alignment of the Box remain the same. See below for why the
        // lifetime change is justified.
        let main =
            unsafe { Box::from_raw(Box::into_raw(main) as *mut (dyn FnOnce() + Send + 'static)) };

        let mut thread_storage = Arc::new(imp::Thread::empty());
        let thread_handle = thread_storage.init_in_place(stack_size, main, priority, core)?;

        let mut res = JoinInner {
            // SAFETY:
            //
            // `imp::Thread::new` takes a closure with a `'static` lifetime, since it's passed
            // through FFI or otherwise used with low-level threading primitives that have no
            // notion of or way to enforce lifetimes.
            //
            // As mentioned in the `Safety` section of this function's documentation, the caller of
            // this function needs to guarantee that the passed-in lifetime is sufficiently long
            // for the lifetime of the thread.
            //
            // Similarly, the `sys` implementation must guarantee that no references to the closure
            // exist after the thread has terminated, which is signaled by `Thread::join`
            // returning.
            native: Pin::new(thread_storage),
            thread: my_thread,
            packet: my_packet,
        };

        // we are still the only running reader/writer since the thread hasn't been started
        // so we can just reach inside the pin as long as we don't cause a move
        unsafe {
            res.thread.inner.set_handle(thread_handle);
        }

        res.native.start()?;
        Ok(res)
    }
}

/// Spawns a new thread, returning a [`JoinHandle`] for it.
///
/// The join handle provides a [`join`] method that can be used to join the spawned
/// thread. If the spawned thread panics, [`join`] will return an [`Err`] containing
/// the argument given to [`panic!`].
///
/// If the join handle is dropped, the spawned thread will implicitly be *detached*.
/// In this case, the spawned thread may no longer be joined.
/// (It is the responsibility of the program to either eventually join threads it
/// creates or detach them; otherwise, a resource leak will result.)
///
/// This call will create a thread using default parameters of [`Builder`], if you
/// want to specify the stack size or the name of the thread, use this API
/// instead.
///
/// As you can see in the signature of `spawn` there are two constraints on
/// both the closure given to `spawn` and its return value, let's explain them:
///
/// - The `'static` constraint means that the closure and its return value
///   must have a lifetime of the whole program execution. The reason for this
///   is that threads can outlive the lifetime they have been created in.
///
///   Indeed if the thread, and by extension its return value, can outlive their
///   caller, we need to make sure that they will be valid afterwards, and since
///   we *can't* know when it will return we need to have them valid as long as
///   possible, that is until the end of the program, hence the `'static`
///   lifetime.
/// - The [`Send`] constraint is because the closure will need to be passed
///   *by value* from the thread where it is spawned to the new thread. Its
///   return value will need to be passed from the new thread to the thread
///   where it is `join`ed.
///   As a reminder, the [`Send`] marker trait expresses that it is safe to be
///   passed from thread to thread. [`Sync`] expresses that it is safe to have a
///   reference be passed from thread to thread.
///
/// # Panics
///
/// Panics if the OS fails to create a thread; use [`Builder::spawn`]
/// to recover from such errors.
///
/// # Examples
///
/// Creating a thread.
///
/// ```
/// use nx::thread;
///
/// let handler = thread::spawn(|| {
///     // thread code
/// });
///
/// handler.join().unwrap();
/// ```
///
/// As mentioned in the module documentation, threads are usually made to
/// communicate using `channels`, here is how it usually looks.
///
/// This example also shows how to use `move`, in order to give ownership
/// of values to a thread.
///
/// ```
/// use nx::thread;
/// use uchan::channel;
///
/// let (tx, rx) = channel();
///
/// let sender = thread::spawn(move || {
///     tx.send("Hello, thread".to_owned())
///         .expect("Unable to send on channel");
/// });
///
/// let receiver = thread::spawn(move || {
///     let value = rx.recv().expect("Unable to receive from channel");
///     println!("{value}");
/// });
///
/// sender.join().expect("The sender thread has panicked");
/// receiver.join().expect("The receiver thread has panicked");
/// ```
///
/// A thread can also return a value through its [`JoinHandle`], you can use
/// this to make asynchronous computations (futures might be more appropriate
/// though).
///
/// ```
/// use nx::thread;
///
/// let computation = thread::spawn(|| {
///     // Some expensive computation.
///     42
/// });
///
/// let result = computation.join().unwrap();
/// println!("{result}");
/// ```
///
/// # Notes
///
/// This function has the same minimal guarantee regarding "foreign" unwinding operations (e.g.
/// an exception thrown from C++ code, or a `panic!` in Rust code compiled or linked with a
/// different runtime) as [`catch_unwind`]; namely, if the thread created with `thread::spawn`
/// unwinds all the way to the root with such an exception, one of two behaviors are possible,
/// and it is unspecified which will occur:
///
/// * The process aborts.
/// * The process does not abort, and [`join`] will return a `Result::Err`
///   containing an opaque type.
///
/// [`catch_unwind`]: unwinding::panic::catch_unwind
/// [`join`]: JoinHandle::join
/// [`Err`]: crate::result::Result::Err
pub fn spawn<F, T>(f: F) -> JoinHandle<T, false>
where
    F: FnOnce() -> T,
    F: Send + 'static,
    T: Send + 'static,
{
    Builder::new().spawn(f).expect("failed to spawn thread")
}

/// Internal wrapper function that is run as the entrypoint to newly spawned functions.
#[doc(hidden)]
#[allow(unsafe_op_in_unsafe_fn)]
unsafe extern "C" fn thread_wrapper(raw_ptr: *mut u8) -> ! {
    // SAFETY: This is fine as it is created with a call to Box::<ThreadArgs>::new()
    let entry_env: Box<ThreadArgs> = Box::from_raw(raw_ptr.cast());

    // SAFETY: The this may actually get mutated by the running thread, so the parent thread *MUST* never read/modify the thread object as it is running
    set_current_thread(&*entry_env.thread.as_ref() as *const _ as _);

    // runs only once and we don't need to handle panics as they're handled with catch_unwind in the runner.
    // The runner is actually a wrapper of the thread payload that captures the thread environment (e.g. the packet for returning data from the thread)
    (entry_env.runner)();

    // access the thread state through the thread local storage, just like the runner would have to.
    current().as_mut().unwrap().state = ThreadState::Terminated;

    svc::exit_thread();
}

struct ThreadArgs {
    runner: Box<dyn FnOnce() + Send + 'static>,
    thread: Pin<Arc<imp::Thread>>,
}

#[derive(Clone)]
#[allow(rustdoc::private_intra_doc_links)]
/// A handle to a thread.
///
/// Threads are represented via the `Thread` type, which you can get in one of
/// two ways:
///
/// * By spawning a new thread, e.g., using the [`thread::spawn`][`spawn`]
///   function, and calling [`thread`][`JoinHandle::thread`] on the
///   [`JoinHandle`].
/// * By requesting the current thread, using the (private, for now) [`thread::current`][`current`] function.
///
/// There is usually no need to create a `Thread` struct yourself, one
/// should instead use a function like `spawn` to create new threads, see the
/// docs of [`Builder`] and [`spawn`] for more details.
pub struct Thread {
    inner: Pin<Arc<Inner>>,
}

impl Thread {
    pub fn new_remote<S: AsRef<str>>(name: S, handle: svc::Handle) -> Thread {
        Self {
            inner: Pin::new(Arc::new(Inner {
                name: name.as_ref().into(),
                thread_handle: UnsafeCell::new(handle),
            })),
        }
    }

    /// # Safety
    /// If `name` is `ThreadName::Other(_)`, the contained string must be valid UTF-8.
    unsafe fn new_inner(name: ThreadName) -> Thread {
        // We have to use `unsafe` here to construct the `Inner` in-place.
        //
        // SAFETY: We pin the Arc immediately after creation, so its address never
        // changes.
        let inner = unsafe {
            let mut arc = Arc::<Inner>::new_uninit();
            let ptr = Arc::get_mut(&mut arc).unwrap_unchecked().as_mut_ptr();
            (&raw mut (*ptr).name).write(name);
            *(*ptr).thread_handle.get_mut() = svc::INVALID_HANDLE;
            Pin::new_unchecked(arc.assume_init())
        };

        Thread { inner }
    }

    /// Gets the thread's unique identifier.
    ///
    /// # Examples
    ///
    /// ```
    /// use crate::thread;
    ///
    /// let other_thread = thread::spawn(|| {
    ///     thread::current().id()
    /// });
    ///
    /// let other_thread_id = other_thread.join().unwrap();
    /// assert!(thread::current().id() != other_thread_id);
    /// ```
    #[must_use]
    pub fn id(&self) -> ThreadId {
        svc::get_thread_id(unsafe { *self.inner.thread_handle.get() }).unwrap()
    }

    /// Gets the thread's name.
    ///
    /// For more information about named threads, see
    /// [this module-level documentation][naming-threads].
    ///
    /// # Examples
    ///
    /// Threads by default have no name specified:
    ///
    /// ```
    /// use crate::thread;
    ///
    /// let builder = thread::Builder::new();
    ///
    /// let handler = builder.spawn(|| {
    ///     assert!(thread::current().name().is_none());
    /// }).unwrap();
    ///
    /// handler.join().unwrap();
    /// ```
    ///
    /// Thread with a specified name:
    ///
    /// ```
    /// use crate::thread;
    ///
    /// let builder = thread::Builder::new()
    ///     .name("foo".into());
    ///
    /// let handler = builder.spawn(|| {
    ///     assert_eq!(thread::current().name(), Some("foo"))
    /// }).unwrap();
    ///
    /// handler.join().unwrap();
    /// ```
    ///
    /// [naming-threads]: ./index.html#naming-threads
    #[must_use]
    pub fn name(&self) -> ThreadName {
        self.inner.name
    }

    /// Atomically makes the handle’s token available if it is not already.
    ///
    /// Takes no effect if the thread was currently unparked, but this marks parked threads for scheduling.
    pub fn unpark(&self) {
        let _ = unsafe {
            svc::set_thread_activity(
                core::ptr::read(self.inner.thread_handle.get()),
                svc::SchedulerState::Runnable,
            )
        };
    }
}

impl fmt::Debug for Thread {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Thread")
            .field("id", &self.id())
            .field("name", &self.name())
            .finish_non_exhaustive()
    }
}

struct Inner {
    name: ThreadName,
    thread_handle: UnsafeCell<svc::Handle>,
}

unsafe impl Sync for Inner {}

impl Inner {
    pub(self) unsafe fn set_handle(&self, handle: svc::Handle) {
        unsafe { *self.thread_handle.get() = handle };
    }
}

////////////////////////////////////////////////////////////////////////////////
// JoinHandle
////////////////////////////////////////////////////////////////////////////////

/// A specialized [`Result`] type for threads.
///
/// Indicates the manner in which a thread exited.
///
/// The value contained in the `Result::Err` variant
/// is the value the thread panicked with;
/// that is, the argument the `panic!` macro was called with.
/// Unlike with normal errors, this value doesn't implement
/// the [`Error`](core::error::Error) trait.
///
/// Thus, a sensible way to handle a thread panic is to either:
///
/// 1. re-raise the panic with [`unwinding::panic::begin_panic`]
/// 2. or in case the thread is intended to be a subsystem boundary
///    that is supposed to isolate system-level failures,
///    match on the `Err` variant and handle the panic in an appropriate way
///
/// A thread that completes without panicking is considered to exit successfully.
///
/// # Examples
///
/// Matching on the result of a joined thread:
///
/// ```no_run
/// use crate::{fs, thread, panic};
///
/// fn copy_in_thread() -> thread::Result<()> {
///     thread::spawn(|| {
///         fs::copy("sdmc://foo.txt", "sdmc://bar.txt").unwrap();
///     }).join()
/// }
///
/// fn main() {
///     fs::initialize_fspsrv_session();
///     fs::mount_sd_card("sdmc");
///
///     match copy_in_thread() {
///         Ok(_) => println!("copy succeeded"),
///         Err(e) => unwinding::panic::begin_panic(e),
///     }
/// }
/// ```
///
/// [`Result`]: core::result::Result
/// [`begin_panic`]: unwinding::panic::begin_panic
pub type Result<T> = core::result::Result<T, Box<dyn core::any::Any + Send + 'static>>;

/// This packet is used to communicate the return value between the spawned
/// thread and the rest of the program. It is shared through an `Arc` and
/// there's no need for a mutex here because synchronization happens with `join()`
/// (the caller will never read this packet until the thread has exited).
///
/// An Arc to the packet is stored into a `JoinInner` which in turns is placed
/// in `JoinHandle`.
struct Packet<'scope, T> {
    scope: Option<Arc<scoped::ScopeData>>,
    result: UnsafeCell<Option<Result<T>>>,
    _marker: PhantomData<Option<&'scope scoped::ScopeData>>,
}

// Due to the usage of `UnsafeCell` we need to manually implement Sync.
// The type `T` should already always be Send (otherwise the thread could not
// have been created) and the Packet is Sync because all access to the
// `UnsafeCell` synchronized (by the `join()` boundary), and `ScopeData` is Sync.
unsafe impl<T: Send> Sync for Packet<'_, T> {}

impl<T> Drop for Packet<'_, T> {
    fn drop(&mut self) {
        // If this packet was for a thread that ran in a scope, the thread
        // panicked, and nobody consumed the panic payload, we make sure
        // the scope function will panic.
        let unhandled_panic = matches!(self.result.get_mut(), Some(Err(_)));

        // Drop the result without causing unwinding.
        // This is only relevant for threads that aren't join()ed, as
        // join() will take the `result` and set it to None, such that
        // there is nothing left to drop here.
        // If this panics, we should handle that, because we're outside the
        // outermost `catch_unwind` of our thread.
        // We just abort in that case, since there's nothing else we can do.
        // (And even if we tried to handle it somehow, we'd also need to handle
        // the case where the panic payload we get out of it also panics on
        // drop, and so on. See issue #86027.)
        if unwinding::panic::catch_unwind(core::panic::AssertUnwindSafe(|| {
            *self.result.get_mut() = None;
        }))
        .is_err()
        {
            abort::abort(AbortLevel::Panic(), crate::rc::ResultPanicked::make());
        }

        // Book-keeping so the scope knows when it's done.
        if let Some(scope) = &self.scope {
            // Now that there will be no more user code running on this thread
            // that can use 'static, mark the thread as 'finished'.
            // It's important we only do this after the `result` has been dropped,
            // since dropping it might still use things it borrowed from 'static.
            scope.decrement_num_running_threads(unhandled_panic);
        }
    }
}

/// Inner representation for JoinHandle
struct JoinInner<'scope, T> {
    native: Pin<Arc<imp::Thread>>,
    thread: Thread,
    packet: Arc<Packet<'scope, T>>,
}

impl<T> Clone for JoinInner<'static, T> {
    fn clone(&self) -> Self {
        Self {
            native: self.native.clone(),
            thread: self.thread.clone(),
            packet: self.packet.clone(),
        }
    }
}

impl<T> JoinInner<'_, T> {
    fn join(&mut self) -> Result<T> {
        let _ = self.native.join();
        Arc::get_mut(&mut self.packet)
            .unwrap()
            .result
            .get_mut()
            .take()
            .unwrap()
    }

    fn wait_exit(&self, timeout: Option<i64>) -> crate::result::Result<()> {
        self.native.join_timeout(timeout)
    }
}

/// An owned permission to join on a thread (block on its termination).
///
/// A `JoinHandle` *detaches* the associated thread when it is dropped, which
/// means that there is no longer any handle to the thread and no way to `join`
/// on it.
///
/// Due to platform restrictions, it is not possible to [`Clone`] this
/// handle: the ability to join a thread is a uniquely-owned permission.
///
/// This `struct` is created by the [`thread::spawn`] function and the
/// [`thread::Builder::spawn`] method.
///
/// # Examples
///
/// Creation from [`thread::spawn`]:
///
/// ```
/// use crate::thread;
///
/// let join_handle: thread::JoinHandle<_> = thread::spawn(|| {
///     // some work here
/// });
/// ```
///
/// Creation from [`thread::Builder::spawn`]:
///
/// ```
/// use crate::thread;
///
/// let builder = thread::Builder::new();
///
/// let join_handle: thread::JoinHandle<_> = builder.spawn(|| {
///     // some work here
/// }).unwrap();
/// ```
///
/// A thread being detached and outliving the thread that spawned it:
///
/// ```no_run
/// use crate::thread;
/// use crate::time::Duration;
///
/// let original_thread = thread::spawn(|| {
///     let _detached_thread = thread::spawn(|| {
///         // Here we sleep to make sure that the first thread returns before.
///         thread::sleep(Duration::from_millis(10));
///         // This will be called, even though the JoinHandle is dropped.
///         println!("♫ Still alive ♫");
///     });
/// });
///
/// original_thread.join().expect("The thread being joined has panicked");
/// println!("Original thread is joined.");
///
/// // We make sure that the new thread has time to run, before the main
/// // thread returns.
///
/// thread::sleep(Duration::from_millis(1000));
/// ```
///
/// [`thread::Builder::spawn`]: Builder::spawn
/// [`thread::spawn`]: spawn
pub struct JoinHandle<T, const BLOCK_ON_DROP: bool = false>(JoinInner<'static, T>);

unsafe impl<T, const BLOCK_ON_DROP: bool> Send for JoinHandle<T, BLOCK_ON_DROP> {}
unsafe impl<T, const BLOCK_ON_DROP: bool> Sync for JoinHandle<T, BLOCK_ON_DROP> {}

impl<T, const BLOCK_ON_DROP: bool> JoinHandle<T, BLOCK_ON_DROP> {
    /// Extracts a handle to the underlying thread.
    ///
    /// # Examples
    ///
    /// ```
    /// use crate::thread;
    ///
    /// let builder = thread::Builder::new();
    ///
    /// let join_handle: thread::JoinHandle<_> = builder.spawn(|| {
    ///     // some work here
    /// }).unwrap();
    ///
    /// let thread = join_handle.thread();
    /// println!("thread id: {:?}", thread.id());
    /// ```
    #[must_use]
    pub fn thread(&self) -> &Thread {
        &self.0.thread
    }

    /// Waits for the associated thread to finish.
    ///
    /// This function will return immediately if the associated thread has already finished.
    ///
    /// In terms of [atomic memory orderings],  the completion of the associated
    /// thread synchronizes with this function returning. In other words, all
    /// operations performed by that thread [happen
    /// before](https://doc.rust-lang.org/nomicon/atomics.html#data-accesses) all
    /// operations that happen after `join` returns.
    ///
    /// If the associated thread panics, [`Err`] is returned with the parameter given
    /// to [`panic!`].
    ///
    /// [`Err`]: crate::result::Result::Err
    /// [atomic memory orderings]: core::sync::atomic
    ///
    /// # Panics
    ///
    /// This function may panic on some platforms if a thread attempts to join
    /// itself or otherwise may create a deadlock with joining threads.
    ///
    /// # Examples
    ///
    /// ```
    /// use crate::thread;
    ///
    /// let builder = thread::Builder::new();
    ///
    /// let join_handle: thread::JoinHandle<_> = builder.spawn(|| {
    ///     // some work here
    /// }).unwrap();
    /// join_handle.join().expect("Couldn't join on the associated thread");
    /// ```
    pub fn join(mut self) -> Result<T> {
        self.0.join()
    }

    /// Waits for the associated thread to finish, with a timeout (in nanoseconds)
    ///
    /// This function will return immediately if the associated thread has already finished.
    ///
    /// In terms of [atomic memory orderings],  the completion of the associated
    /// thread synchronizes with this function returning. In other words, all
    /// operations performed by that thread [happen
    /// before](https://doc.rust-lang.org/nomicon/atomics.html#data-accesses) all
    /// operations that happen after `join` returns.
    ///
    /// If the associated thread panics, [`Err`] is returned with the parameter given
    /// to [`panic!`].
    ///
    /// [`Err`]: crate::result::Result::Err
    /// [atomic memory orderings]: core::sync::atomic
    ///
    /// # Panics
    ///
    /// This function may panic on some platforms if a thread attempts to join
    /// itself or otherwise may create a deadlock with joining threads.
    ///
    /// # Examples
    ///
    /// ```
    /// use crate::thread;
    ///
    /// let builder = thread::Builder::new();
    ///
    /// let join_handle: thread::JoinHandle<_> = builder.spawn(|| {
    ///     // some work here
    /// }).unwrap();
    /// join_handle.join().expect("Couldn't join on the associated thread");
    /// ```
    pub fn wait_exit(&self, timeout: Option<i64>) -> crate::result::Result<()> {
        self.0.wait_exit(timeout)
    }

    /// Checks if the associated thread has finished running its main function.
    ///
    /// `is_finished` supports implementing a non-blocking join operation, by checking
    /// `is_finished`, and calling `join` if it returns `true`. This function does not block. To
    /// block while waiting on the thread to finish, use [`join`][Self::join].
    ///
    /// This might return `true` for a brief moment after the thread's main
    /// function has returned, but before the thread itself has stopped running.
    /// However, once this returns `true`, [`join`][Self::join] can be expected
    /// to return quickly, without blocking for any significant amount of time.
    pub fn is_finished(&self) -> bool {
        Arc::strong_count(&self.0.packet) == 1
    }
}

impl<T, const BLOCK_ON_DROP: bool> Drop for JoinHandle<T, BLOCK_ON_DROP> {
    fn drop(&mut self) {
        if BLOCK_ON_DROP {
            let _ = self.wait_exit(None);
        }
    }
}

impl<T, const BLOCK_ON_DROP: bool> util::AsInner<imp::Thread> for JoinHandle<T, BLOCK_ON_DROP> {
    fn as_inner(&self) -> &imp::Thread {
        &self.0.native
    }
}

/*
impl<T, const BLOCK_ON_DROP: bool> util::IntoInner<Arc<imp::Thread>> for JoinHandle<T, BLOCK_ON_DROP> {
    fn into_inner(self) -> Arc<imp::Thread> {
        Pin::into_inner(self.0.native)
    }
} */

impl<T, const BLOCK_ON_DROP: bool> fmt::Debug for JoinHandle<T, BLOCK_ON_DROP> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("JoinHandle").finish_non_exhaustive()
    }
}

fn _assert_sync_and_send() {
    fn _assert_both<T: Send + Sync>() {}
    _assert_both::<JoinHandle<(), false>>();
    _assert_both::<Thread>();
}

pub fn available_parallelism() -> usize {
    4usize
}

/////////////////////////////////
/// Represents a [`Thread`]'s name, a 32-byte `ArrayString`
pub type ThreadName = util::ArrayString<0x20>;

/// Represents the state of a [`Thread`]
#[derive(Copy, Clone, PartialEq, Eq, Debug, Default)]
#[repr(u8)]
pub enum ThreadState {
    #[default]
    NotInitialized = 0,
    Initialized = 1,
    DestroyedBeforeStarted = 2,
    Started = 3,
    Terminated = 4,
}

/// Represents what core the (thread)[`Thread`] should start on
#[derive(Copy, Clone, PartialEq, Eq, Debug, Default)]
#[repr(i32)]
pub enum ThreadStartCore {
    #[default]
    Default = -2,
    Core0 = 0,
    Core1 = 1,
    Core2 = 2,
    Core3 = 3,
}

/// Represents the priority of the (thread)[`Thread`] that we are spawning
#[derive(Copy, Clone, PartialEq, Eq, Debug, Default)]
#[repr(i32)]
pub enum ThreadPriority {
    /// Inherit the thread priority from the current thread
    #[default]
    Inherit,
    /// Use the default priority for the main thread (0x2C)
    Default,
    /// Set and explicit thread priority value (0..=0x3F)
    Set(i32),
}

impl ThreadPriority {
    pub fn set(value: i32) -> crate::result::Result<Self> {
        Self::try_from(value)
    }

    /// Returns the effective thread priority as the raw type (`i32`)
    pub fn to_raw(self) -> i32 {
        match self {
            Self::Inherit => {
                svc::get_thread_priority(svc::CURRENT_THREAD_PSEUDO_HANDLE).unwrap_or(0x2C)
            }
            Self::Default => 0x2C,
            Self::Set(v) => v,
        }
    }
}

impl TryFrom<i32> for ThreadPriority {
    type Error = ResultCode;
    fn try_from(value: i32) -> crate::result::Result<Self> {
        if (0i32..=0x3F).contains(&value) {
            Ok(Self::Set(value))
        } else {
            Err(rc::ResultInvalidPriority::make())
        }
    }
}

pub mod imp {
    use core::{
        alloc::{Allocator, Layout},
        pin::Pin,
        ptr::{NonNull, null, null_mut},
    };

    use alloc::{alloc::Global, boxed::Box, sync::Arc};

    use super::{
        ThreadArgs, ThreadId, ThreadName, ThreadPriority, ThreadStartCore, ThreadState,
        thread_wrapper,
    };
    use crate::{mem::alloc::PAGE_ALIGNMENT, svc, util::ArrayString, wait::wait_handles};

    pub type Thread = StratosphereThreadType;

    /// A replica of libstratosphere's named threads
    #[derive(Debug)]
    #[repr(C)]
    pub struct StratosphereThreadType {
        /// An intrusive linked list header for the global thread list
        __intrusive_thread_list_node: [usize; 2],
        // doubly linked list of waiters on the thread
        __thread_wait_list: [usize; 2],
        // reserved field
        _reserved: [usize; 4],
        // current state of the thread
        pub(crate) state: ThreadState,
        // flag whether the stack memory has been remapped
        stack_is_aliased: bool,
        // auto_registered??
        _auto_registered: bool,
        // count of thread suspensions?
        _suspend_count: u8,
        //magic value snuck in between the official fields' padding, should always be 0xF5A5
        magic: u16,
        //base_priority: thread priority when not parked?
        _base_priority: i16,
        // thread version
        version: u16,
        // name of the thread,
        pub(crate) name: ThreadName,
        // pointer to the previous field (thread must not move after write)
        pub(crate) name_ptr: *const u8,
        // thread ID
        id: ThreadId,
        // original stack address
        original_stack_top: *mut u8,
        // aliased stack address (if stack_is_aliased == true)
        pub(crate) stack_top: *mut u8,
        // size of the stack
        stack_size: usize,
        // pointer to the thread function to run. Since we know that function pointers can't be null, we can have an
        // option that is the same through niche optimization. This lets us initialize it late to the "main" wrapper on
        // the spawned thread.
        entry: Option<super::svc::ThreadEntrypointFn>,
        // initial fiber - not used as we don't support lightweight threads
        _initial_fiber: *mut (),
        // current_fiber - not used as we don't support lightweight threads
        _current_fiber: *mut (),
        // arguments to the function we're running - pointer must have a longer lifetime than the Thread object
        pub(crate) arguments: *const u8,
        // storage of an internal critical section (u32 in libatmosphere's impl for Horizon)
        _internal_critical_section_storage: u32,
        // storage of an internal conditional variable for signaling between threads? (u32 in libatmosphere's impl for Horizon)
        _internal_condvar_storage: u32,
        // internal thread storage pointer
        _nn_sdk_internal_tls_type: *mut (),
        // pointer to the next member. Libnx stores it at the top of the stack, but we're keeping it
        // here because that's what stratosphere does.
        pub(crate) __nx_thread_pointer: *const LibNxThread,
        // libnx thread type, we'll store this to be compatible with libnx/stratosphere threads
        pub(crate) __nx_thread: LibNxThread,
    }

    unsafe impl Send for StratosphereThreadType {}
    unsafe impl Sync for StratosphereThreadType {}

    impl StratosphereThreadType {
        pub(crate) const MAGIC: u16 = 0xF5A5;

        pub(crate) fn join(&self) -> crate::result::Result<()> {
            wait_handles(&[self.__nx_thread.handle], -1).map(|_| ())
        }

        pub(crate) fn join_timeout(&self, timeout: Option<i64>) -> crate::result::Result<()> {
            wait_handles(&[self.__nx_thread.handle], timeout.unwrap_or(-1)).map(|_| ())
        }

        pub fn name(&self) -> ThreadName {
            self.name
        }
        pub fn set_name<S: AsRef<str>>(&mut self, name: S) {
            self.name = name.as_ref().into()
        }

        pub fn handle(&self) -> svc::Handle {
            self.__nx_thread.handle
        }

        pub fn id(&self) -> crate::result::Result<ThreadId> {
            svc::get_thread_id(self.handle())
        }

        pub(crate) const fn empty() -> Self {
            Self {
                __intrusive_thread_list_node: [0, 0],
                __thread_wait_list: [0, 0],
                _reserved: [0; 4],
                state: ThreadState::NotInitialized,
                stack_is_aliased: false,
                _auto_registered: false,
                _suspend_count: 0,
                magic: Self::MAGIC,
                _base_priority: 0x2C,
                version: 1,
                name: ArrayString::new(),
                name_ptr: null(),
                id: 0,
                original_stack_top: null_mut(),
                stack_top: null_mut(),
                stack_size: 0,
                entry: None,
                _initial_fiber: null_mut(),
                _current_fiber: null_mut(),
                arguments: null(),
                _internal_critical_section_storage: 0,
                _internal_condvar_storage: 0,
                _nn_sdk_internal_tls_type: null_mut(),
                __nx_thread_pointer: null(),
                __nx_thread: LibNxThread {
                    handle: svc::INVALID_HANDLE,
                    _owns_stack_mem: true,
                    _stack_mem: null_mut(),
                    _stack_mirror: null_mut(),
                    _stack_sz: 0,
                    _tls_array: null_mut(),
                    _next: null_mut(),
                    _prev_next: null_mut(),
                },
            }
        }

        pub(crate) unsafe fn init_in_place(
            self: &mut Arc<Self>,
            stack_size: usize,
            main: Box<dyn FnOnce() + Send + 'static>,
            priority: ThreadPriority,
            core: ThreadStartCore,
        ) -> crate::result::Result<svc::Handle> {
            // stack sized up to be a multiple of a page
            let aligned_stack_size = align_up!(stack_size, PAGE_ALIGNMENT);
            // layout for stack allocation, we know it's safe because the page alignment constant is a valid alignment
            let stack_layout =
                unsafe { Layout::from_size_align_unchecked(aligned_stack_size, PAGE_ALIGNMENT) };
            // now we can request memory for the stack
            let stack: *mut u8 = Global.allocate(stack_layout).unwrap().as_ptr().cast();
            // this is technically out of bounds, but the stack top should point to the byte *AFTER* the end of the stack not the last byte of the stack.
            let stack_top = unsafe { stack.add(aligned_stack_size) };

            let entry_args = Box::new(ThreadArgs {
                runner: main,
                thread: Pin::new(self.clone()),
            });
            let entry_args_raw = Box::into_raw(entry_args) as *mut u8;
            let handle = match unsafe {
                svc::create_thread(
                    thread_wrapper,
                    entry_args_raw,
                    stack_top,
                    priority.to_raw(),
                    core as _,
                )
            } {
                Ok(handle) => handle,
                Err(e) => {
                    unsafe {
                        let _reclaimed_thread_args =
                            Box::from_raw(entry_args_raw as *mut ThreadArgs);

                        Global.deallocate(NonNull::new_unchecked(stack), stack_layout);
                    }
                    return Err(e);
                }
            };

            unsafe {
                // SAFETY: we don't cause any moves here, so the pin invariant is met.
                // This is breaking mutation-xor-sharing technically, but we know that the other handle to the thread data can't access yet as the new thread hasn't started.
                // Additionally, we need to do `get_mut_unchecked` here as `get_mut().unwrap{_unchecked}()` will always fail (since the strong and weak counts will always be 2).
                let arc = Arc::get_mut_unchecked(self);

                // we can only do this when the remote thread has not yet started though
                debug_assert!(arc.state == ThreadState::NotInitialized);

                arc.__nx_thread.handle = handle;
                arc.entry = Some(thread_wrapper);
                arc.arguments = entry_args_raw;
                arc.name_ptr = (&raw const arc.name).cast();
                arc.__nx_thread_pointer = &raw const arc.__nx_thread;
                arc.state = ThreadState::Initialized;
            }

            Ok(handle)
        }

        pub(crate) fn start(self: &mut Pin<Arc<Self>>) -> crate::result::Result<()> {
            svc::start_thread(self.handle())?;
            Ok(())
        }
    }

    impl Drop for StratosphereThreadType {
        fn drop(&mut self) {
            if !self.stack_top.is_null() {
                // local thread
                unsafe {
                    Global.deallocate(
                        NonNull::new_unchecked(self.stack_top.sub(self.stack_size)),
                        Layout::from_size_align_unchecked(self.stack_size, PAGE_ALIGNMENT),
                    );
                }
                if !self.arguments.is_null() // thread arguments were set
                && (self.state == ThreadState::DestroyedBeforeStarted || self.state == ThreadState::Initialized)
                // thread wasn't started so the runner wouldn't have claimed the pointer
                {
                    // drop the allocated, but unclaimed, pointer to the thread start arguments
                    unsafe { drop(Box::from_raw(self.arguments as *mut u8 as *mut ThreadArgs)) };
                }
            }
        }
    }

    #[derive(Debug)]
    pub(crate) struct LibNxThread {
        /// Thread handle
        pub handle: u32,
        /// Whether the thread should clean up it's stack when it
        _owns_stack_mem: bool,
        /// Pointer to stack memory.
        _stack_mem: *mut u8,
        /// Pointer to stack memory mirror.
        _stack_mirror: *mut u8,
        /// Stack size.
        _stack_sz: usize,
        // array of thread local objects
        _tls_array: *mut *mut (),
        // pointer to next thread in doubly linked list of current threads
        _next: *mut Self,
        // pointer to previous thread in doubly linked list of current threads
        _prev_next: *mut *mut Self,
    }

    #[derive(Clone, Debug)]
    #[repr(C)]
    pub struct LibNxThreadVars {
        // Thread Handle
        pub handle: u32,
        // Magic val: !TV$,
        pub magic: u32,
        // thread pointer
        pub thread_ref: *mut StratosphereThreadType,
        pub _reent_ptr: *mut (),
        pub _tls_tp: *mut (),
    }
    const_assert!(core::mem::size_of::<LibNxThreadVars>() == 0x20);
    impl LibNxThreadVars {
        pub const MAGIC: u32 = u32::from_le_bytes(*b"!TV$");
    }
}

/// Represents the console's Thread Local Region layout
#[derive(Clone, Debug)]
#[repr(C)]
pub struct ThreadLocalRegion {
    /// The region used for IPC messages
    pub msg_buffer: [u8; 0x100],
    /// The disabled counter
    pub _disable_counter: u16,
    /// The interrupt flag
    pub _interrupt_flag: u16,
    pub cache_maintenance_flag: bool, // HOS v14.0.0.+
    pub _reserved_1: [u8; 0x3],
    // These we are ignoring since we're going to use the libnx threadVars anyway and just not use anything in this region
    /*pub reserved_1: [u8; 0x4],
    pub reserved_2: [u8; 0x78],
    // use u32s as that is the required alignment
    pub tls: [u32; 0x50/size_of::<u32>()],
    pub locale_ptr: *mut u8,
    pub errno_val: i64,
    pub thread_data: [u8; 0x8],
    pub eh_globals: [u8; 0x8],
    pub thread_ptr: *mut u8,
    /// The region we (and Nintendo) use to store the current [`Thread`] reference
    pub thread_ref: *mut imp::StratosphereThreadType,*/
    pub _ignored: [u8; (0x200 - core::mem::size_of::<LibNxThreadVars>()) - /*start offset*/ 0x108],
    pub nx_thread_vars: LibNxThreadVars,
}
const_assert!(core::mem::size_of::<ThreadLocalRegion>() == 0x200);
const_assert!(core::mem::align_of::<ThreadLocalRegion>() >= 4); // for some assumptions in the IPC code

/// Gets the current thread's [`ThreadLocalRegion`] address
#[inline(always)]
pub fn get_thread_local_region() -> *mut ThreadLocalRegion {
    #[unsafe(naked)]
    unsafe extern "C" fn __nx_thread_get_thread_local_region() -> *mut ThreadLocalRegion {
        core::arch::naked_asm!(
            maybe_cfi!(".cfi_startproc"),
            "mrs x0, tpidrro_el0",
            "ret",
            maybe_cfi!(".cfi_endproc")
        )
    }
    unsafe { __nx_thread_get_thread_local_region() }
}

pub(crate) unsafe fn current() -> *mut imp::Thread {
    unsafe { (*get_thread_local_region()).nx_thread_vars.thread_ref }
}

/// Sets the current [`Thread`] reference on the current [`ThreadLocalRegion`]
///
/// This is internally used when launching a [`Thread`], and probably shouldn't be used manually
///
/// # Arguments
///
/// * `thread_ref`: The [`Thread`] address to set
/// # Safety
///
/// `thread_ref`` must be a valid Thread pointer that will not move until the thread is finished running.
pub unsafe fn set_current_thread(thread_ref: *mut imp::Thread) {
    unsafe {
        (*thread_ref).state = ThreadState::Started;
        (*thread_ref).name_ptr = (&raw const (*thread_ref).name).cast();
        let tlr = get_thread_local_region();
        debug_assert!(
            !tlr.is_null(),
            "tlr should always be valid, as the kernel sets tpidrro_el0 on context switch, and creates the TLR for us."
        );
        (*tlr).nx_thread_vars.thread_ref = thread_ref;
        (*tlr).nx_thread_vars.handle = (*thread_ref).__nx_thread.handle;
        (*tlr).nx_thread_vars.magic = imp::LibNxThreadVars::MAGIC;
    }
}

pub fn set_current_thread_name(s: impl AsRef<str>) {
    // SAFETY - The TLR is set up in rrt0, or in the thread startup code - we can only reach hear when the TLR is initialized
    unsafe {
        (*(*get_thread_local_region()).nx_thread_vars.thread_ref).set_name(s);
    }
}

pub fn get_current_thread_name() -> String {
    // SAFETY - The TLR is set up in rrt0, or in the thread startup code - we can only reach hear when the TLR is initialized
    unsafe {
        (*(*get_thread_local_region()).nx_thread_vars.thread_ref)
            .name
            .get_str()
            .unwrap_or("")
            .to_string()
    }
}

/// Sleeps for the given timeout
///
/// Essentially a wrapper for [`svc::sleep_thread`]
///
/// # Arguments
///
/// * `timeout`: Sleep timeout in nanoseconds, where `0` can be used for yielding without core migration, `-1` for yielding with core migration and `-2` for yielding to any other thread
#[inline]
pub fn sleep(timeout: i64) -> crate::result::Result<()> {
    svc::sleep_thread(timeout)
}

/// Represents the thread yielding types for cooperative multitasking
#[repr(i64)]
pub enum YieldType {
    /// Yields to another thread on the same core
    WithoutCoreMigration = 0,
    /// Yields to another thread (possibly on a different core)
    WithCoreMigration = -1,
    /// Yields and performs forced load-balancing
    ToAnyThread = -2,
}

#[inline]
pub fn r#yield(yield_type: YieldType) -> crate::result::Result<()> {
    svc::sleep_thread(yield_type as i64)
}

/// Exits the current thread
///
/// Essentially a wrapper for [`svc::exit_thread`]
#[inline]
pub fn exit() -> ! {
    svc::exit_thread()
}

/// Blocks unless or until the current thread’s token is made available.
///
/// A call to `park` does not guarantee that the thread will remain parked forever,
/// and callers should be prepared for this possibility.
/// However, it is guaranteed that this function will not panic.
pub fn park() {
    let current_thread = unsafe { current().as_mut().unwrap() };
    let _ = svc::set_thread_activity(current_thread.handle(), svc::SchedulerState::Paused);
}

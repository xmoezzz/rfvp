//! Library applet support and utils

use crate::arm;
use crate::ipc::sf;
use crate::result::*;
use crate::service::applet;
use crate::service::applet::ILibraryAppletAccessorClient;
use crate::service::applet::ILibraryAppletCreatorClient;
use crate::service::applet::{IStorageAccessorClient, IStorageClient, Storage};
use crate::service::sm::rc;
use crate::svc;
use crate::sync::{Mutex, MutexGuard};
use crate::wait;
use alloc::boxed::Box;
use applet::LibraryAppletCreator;
use core::mem as cmem;

/// Represents the common arguments layout sent as starting input by/to all library applets
#[derive(Copy, Clone, PartialEq, Eq, Debug, Default)]
#[repr(C)]
pub struct CommonArguments {
    /// Represents the [`CommonArguments`] version
    ///
    /// Usually value `1` is used
    pub version: u32,
    /// [`CommonArguments`] size (essentially the [`size_of`][`cmem::size_of`] this struct)
    pub size: u32,
    /// Represents the API version of the specific library applet being launched
    pub la_api_version: u32,
    /// Represents the theme color for the library applet to use
    pub theme_color: u32,
    /// Represents whether the library applet should make a startup sound when launched
    pub play_startup_sound: bool,
    /// Padding bytes
    pub pad: [u8; 7],
    /// Represents the system tick of when the library applet gets launched
    pub system_tick: u64,
}

/// Represents a wrapper type for using library applets
pub struct LibraryAppletHolder {
    accessor: Box<dyn ILibraryAppletAccessorClient>,
    state_changed_event_handle: svc::Handle,
}

impl LibraryAppletHolder {
    /// Creates a [`LibraryAppletHolder`] from an existing [`ILibraryAppletAccessorClient`] shared object
    ///
    /// This shouldn't be manually created unless the accessor object was obtained manually (see [`create_library_applet`])
    pub fn new(mut accessor: Box<dyn ILibraryAppletAccessorClient>) -> Result<Self> {
        let state_changed_event_h = accessor.get_applet_state_changed_event()?;

        Ok(Self {
            accessor,
            state_changed_event_handle: state_changed_event_h.handle,
        })
    }

    /// Gets a reference to the underlying [`ILibraryAppletAccessorClient`] shared object
    #[inline]
    pub fn get_accessor(&self) -> &dyn ILibraryAppletAccessorClient {
        &*self.accessor
    }

    /// Gets a mutable reference to the underlying [`ILibraryAppletAccessorClient`] shared object
    #[inline]
    pub fn get_accessor_mut(&mut self) -> &dyn ILibraryAppletAccessorClient {
        &mut *self.accessor
    }

    /// Pushes an input [`IStorageClient`] shared object to the library applet
    #[inline]
    pub fn push_in_data_storage(&mut self, storage: Storage) -> Result<()> {
        self.accessor.push_in_data(storage)
    }

    /// Pushes input data to the library applet
    ///
    /// This is a wrapper which creates an [`IStorageClient`] object with the given value and pushes it
    pub fn push_in_data<T: Copy>(&mut self, t: T) -> Result<()> {
        let t_st = create_write_storage(t)?;
        self.push_in_data_storage(t_st)
    }

    /// Starts the library applet
    #[inline]
    pub fn start(&mut self) -> Result<()> {
        self.accessor.start()
    }

    /// Waits until the library applet's state-changed event signals
    ///
    /// This effectively waits until the library applet exits or the timeout expires
    #[inline]
    pub fn join(&mut self, timeout: Option<i64>) -> Result<()> {
        wait::wait_handles(&[self.state_changed_event_handle], timeout.unwrap_or(-1))?;
        Ok(())
    }

    /// Pops an output [`IStorageClient`] shared object from the library applet
    #[inline]
    pub fn pop_out_data_storage(&mut self) -> Result<Storage> {
        self.accessor.pop_out_data()
    }

    /// Pops output data from the library applet
    ///
    /// This is a wrapper which pops an [`IStorageClient`] object and reads its data (reads [`size_of`][`cmem::size_of`] `O` bytes and returns that data)
    pub fn pop_out_data<O: Copy>(&mut self) -> Result<O> {
        let mut o_st = self.pop_out_data_storage()?;
        read_storage(&mut o_st)
    }
}

impl Drop for LibraryAppletHolder {
    /// Drops the [`LibraryAppletHolder`], closing the [`ILibraryAppletAccessorClient`] object instance and the acquired state-changed event handle
    fn drop(&mut self) {
        let _ = svc::close_handle(self.state_changed_event_handle);
    }
}

static G_CREATOR: Mutex<Option<LibraryAppletCreator>> = Mutex::new(None);

/// Initializes library applet support with the provided [`LibraryAppletCreator`]
///
/// # Arguments
///
/// * `creator`: The shared object to use globally
#[inline]
pub fn initialize(creator: LibraryAppletCreator) {
    *G_CREATOR.lock() = Some(creator);
}

/// Gets whether library applet support was initialized
#[inline]
pub fn is_initialized() -> bool {
    G_CREATOR.lock().is_some()
}

/// Finalizes library applet support, dropping the inner [`ILibraryAppletCreatorClient`] shared object instance. Gets run in the rrt0 runtime after the main function runs.
#[inline]
pub(crate) fn finalize() {
    *G_CREATOR.lock() = None;
}

/// Gets access to the global [`ILibraryAppletCreatorClient`] shared object instance
#[inline]
pub fn get_creator<'a>() -> MutexGuard<'a, Option<LibraryAppletCreator>> {
    G_CREATOR.lock()
}

/// Wrapper for reading data from a [`IStorageClient`] shared object
///
/// This will try to read [`size_of`][`cmem::size_of`] `T` bytes from the storage and return them as the expected value
///
/// # Arguments
///
/// * `storage`: The storage to read from
pub fn read_storage<T: Copy>(storage: &mut Storage) -> Result<T> {
    let mut t = unsafe { cmem::zeroed::<T>() };

    let storage_accessor = storage.open()?;
    storage_accessor.read(0, sf::Buffer::from_other_mut_var(&mut t))?;

    Ok(t)
}

/// Wrapper for writing data to a [`IStorageClient`] shared object
///
/// This will try to write [`size_of`][`cmem::size_of`] `T` bytes to the storage from the given value
///
/// # Arguments
///
/// * `storage`: The storage to write to
/// * `t`: The value to write
pub fn write_storage<T: Copy>(storage: &mut Storage, t: T) -> Result<()> {
    result_return_unless!(is_initialized(), super::rc::ResultNotInitialized);

    let storage_accessor = storage.open()?;
    storage_accessor.write(0, sf::Buffer::from_other_var(&t))?;

    Ok(())
}

/// Wrapper for creating a [`IStorageClient`] shared object from the given value
///
/// This will fail with [`ResultNotInitialized`][`super::rc::ResultNotInitialized`] if library applet support isn't initialized
///
/// This will create a [`IStorageClient`] object using the global [`ILibraryAppletCreatorClient`] object and write the given value to it
///
/// # Arguments
///
/// * `t`: The value to write
pub fn create_write_storage<T: Copy>(t: T) -> Result<Storage> {
    result_return_unless!(is_initialized(), super::rc::ResultNotInitialized);

    let mut storage = get_creator()
        .as_ref()
        .ok_or(rc::ResultNotInitialized::make())?
        .create_storage(cmem::size_of::<T>())?;
    write_storage(&mut storage, t)?;

    Ok(storage)
}

/// Creates a [`LibraryAppletHolder`] from the given library applet params
///
/// This automatically sets the [`CommonArguments`] `system_tick` value to the current system tick and pushes it as input using [`push_in_data`][`LibraryAppletHolder::push_in_data`]
///
/// # Arguments
///
/// * `id`: The [`AppletId`][`applet::AppletId`] of the library applet to create
/// * `mode`: The [`LibraryAppletMode`][`applet::LibraryAppletMode`] to create the library applet with
/// * `common_args`: The library applet-specific [`CommonArguments`] to send as input
pub fn create_library_applet(
    id: applet::AppletId,
    mode: applet::LibraryAppletMode,
    mut common_args: CommonArguments,
) -> Result<LibraryAppletHolder> {
    result_return_unless!(is_initialized(), super::rc::ResultNotInitialized);

    let accessor = get_creator()
        .as_ref()
        .ok_or(rc::ResultNotInitialized::make())?
        .create_library_applet(id, mode)?;

    let mut holder = LibraryAppletHolder::new(Box::new(accessor))?;

    common_args.system_tick = arm::get_system_tick();
    holder.push_in_data(common_args)?;

    Ok(holder)
}

/// Wrapper to create, launch and wait for a library applet, expecting simple input and output data
///
/// The mode used (since all simple library applets expect it) is [`LibraryAppletMode::AllForeground`][`applet::LibraryAppletMode::AllForeground`]
///
/// Note that this won't be useful, for instance, with library applets taking interactive in/out data, like [`AppletId::LibraryAppletSwkbd`][`applet::AppletId::LibraryAppletSwkbd`]
///
/// # Arguments
///
/// * `id`: The [`AppletId`][`applet::AppletId`] of the library applet to create
/// * `common_args`: The library applet-specific [`CommonArguments`] to send as input
/// * `input`: The only input data to send after the [`CommonArguments`]
pub fn launch_wait_library_applet<I: Copy, O: Copy>(
    id: applet::AppletId,
    common_args: CommonArguments,
    input: I,
    timeout: Option<i64>,
) -> Result<O> {
    let mut holder =
        create_library_applet(id, applet::LibraryAppletMode::AllForeground, common_args)?;
    holder.push_in_data(input)?;
    holder.start()?;
    holder.join(timeout)?;
    holder.pop_out_data()
}

// TODO: specific library applet implementations in submodules (err, psel, swkbd, etc.)

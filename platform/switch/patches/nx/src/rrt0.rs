//! Initial code/entrypoint support and utils
//!
//! # Custom entrypoint
//!
//! If you wish to define your custom entrypoint, you can do so by redefining the `__nx_rrt0_entry` weak fn.
//!
//! Example (check [here](https://switchbrew.org/wiki/Homebrew_ABI#Entrypoint_Arguments) for more entrypoint details):
//! ```
//! #[unsafe(no_mangle)]
//! unsafe extern "C" fn __nx_rrt0_entry(arg0: usize, arg1: usize) {
//!     // ...
//! }
//! ```
//!
//! # Custom version setup
//!
//! On the default entrypoint routine, the internal system version (see [`get_version`][`version::get_version`] and [`set_version`][`version::set_version`]) gets set the following way:
//! * If the process was launched through HBL, use the "HOS version" value we got from it
//! * Otherwise (and if using the `services` feature), use settings services ([`SystemSettingsServer`][`crate::service::set::ISystemSettingsServer`]) to get it
//!
//! If you wish to define your custom version setup (for instance, in contexts in which you wish to avoid executing the aforementioned setup), you can do so by redefining the `initialize_version` weak fn.
//!
//! Example:
//! ```
//! #[unsafe(no_mangle)]
//! fn initialize_version(hbl_hos_version_opt: Option<hbl::Version>) {
//!     // ...
//! }
//! ```

use crate::elf;
use crate::hbl;
use crate::hbl::AbiConfigEntry;
use crate::mem::alloc;
use crate::result::*;
use crate::svc;
use crate::svc::Handle;
use crate::sync;
use crate::thread;
use crate::thread::get_thread_local_region;
use crate::util;
use crate::version;
use crate::vmem;

#[cfg(feature = "services")]
use crate::{ipc::sf, service, service::set};

use core::arch::asm;
use core::mem;
use core::ptr;
use core::sync::atomic::AtomicUsize;

use atomic_enum::atomic_enum;

// These functions must be implemented by any binary using this crate

unsafe extern "Rust" {
    fn main();
    fn initialize_heap(hbl_heap: util::PointerAndSize) -> util::PointerAndSize;
}

/// Represents the fn pointer used for exiting
pub type ExitFn = unsafe extern "C" fn(ResultCode) -> !;

#[atomic_enum]
/// Represents the executable type of the current process
#[derive(PartialEq, Eq, Default)]
pub enum ExecutableType {
    #[default]
    None,
    Nso,
    Nro,
}

static G_EXECUTABLE_TYPE: AtomicExecutableType = AtomicExecutableType::new(ExecutableType::None);

pub(crate) fn set_executable_type(exec_type: ExecutableType) {
    G_EXECUTABLE_TYPE.store(exec_type, core::sync::atomic::Ordering::SeqCst);
}

/// Gets the current process's executable type
///
/// Note that this can be used to determine if this process was launched through HBL or not (if so, we would be a homebrew NRO and this would return [`ExecutableType::Nro`])
pub fn get_executable_type() -> ExecutableType {
    G_EXECUTABLE_TYPE.load(core::sync::atomic::Ordering::Relaxed)
}

/// Represents the process module format used by processes
///
/// This layout has to be present at the start of the process's `.rodata` section, containing its module name
#[derive(Copy, Clone)]
#[repr(C)]
pub struct ModulePath {
    /// Unused value
    _zero: u32,
    /// The length of the module name
    path_len: u32,
    /// The module name string
    path: util::ArrayString<0x200>,
}

impl ModulePath {
    /// Creates a [`ModulePath`] with the given module name
    ///
    /// # Arguments
    ///
    /// * `name`: The module name
    #[inline]
    pub const fn new(name: &str) -> Self {
        Self {
            _zero: 0,
            path_len: util::const_usize_min(name.len(), 0x200 - 1) as u32,
            path: util::ArrayString::from_str_truncate_null(name),
        }
    }

    pub fn set_name(&mut self, new_name: &str) {
        self.path = util::ArrayString::from_str(new_name);
        self.path_len = new_name.len() as u32
    }

    pub fn get_name(&self) -> util::ArrayString<0x200> {
        self.path
    }
}

#[used]
#[linkage = "weak"]
#[unsafe(link_section = ".module_name")]
#[unsafe(export_name = "__nx_rrt0_module_name")]
static G_MODULE_NAME: ModulePath = ModulePath::new("aarch64-switch-rs (unknown module)");

/// Gets this process's module name
///
/// The module name is `aarch64-switch-rs (unknown module)` by default, but it can be set to a custom one with [`rrt0_define_module_name`] or [`rrt0_define_default_module_name`] macros
pub fn get_module_name() -> ModulePath {
    G_MODULE_NAME
}

static G_EXIT_FN: sync::Mutex<Option<ExitFn>> = sync::Mutex::new(None);
static G_MAIN_THREAD: sync::Mutex<Option<thread::Thread>> = sync::Mutex::new(None);
pub(crate) static TEXT_BASE_ADDRESS: AtomicUsize = AtomicUsize::new(0);
static EH_FRAME_HDR_SECTION: elf::EhFrameHdrPtr = elf::EhFrameHdrPtr::new();

/// Exits the current process
///
/// This will call the HBL-specific exit fn if running as a homebrew NRO, or [`exit_process`][`svc::exit_process`] otherwise
pub fn exit(rc: ResultCode) -> ! {
    match *G_EXIT_FN.lock() {
        Some(exit_fn) => unsafe { (exit_fn)(rc) },
        None => svc::exit_process(),
    }
}

#[linkage = "weak"]
#[unsafe(export_name = "__nx__rrt0_initialize_version")]
pub fn initialize_version(hbl_hos_version_opt: Option<hbl::Version>) {
    if let Some(hbl_hos_version) = hbl_hos_version_opt {
        unsafe { version::set_version(hbl_hos_version.to_version()) };
    } else {
        #[cfg(feature = "services")]
        {
            use crate::service::set::{ISystemSettingsClient, SystemSettingsService};
            let set_sys = service::new_service_object::<SystemSettingsService>().unwrap();
            let mut fw_version: set::FirmwareVersion = Default::default();
            set_sys
                .get_firmware_version(sf::Buffer::from_mut_var(&mut fw_version))
                .unwrap();

            let version =
                version::Version::new(fw_version.major, fw_version.minor, fw_version.micro);
            unsafe { version::set_version(version) };
        }
    }
}

static mut MAIN_THREAD: thread::imp::Thread = thread::imp::Thread::empty();

#[inline]
unsafe fn set_main_thread_tlr(handle: svc::Handle) {
    unsafe {
        let tlr_raw = get_thread_local_region();
        (*tlr_raw).nx_thread_vars.handle = handle;
        (*tlr_raw).nx_thread_vars.magic = thread::imp::LibNxThreadVars::MAGIC;

        MAIN_THREAD.__nx_thread.handle = handle;

        (*tlr_raw).nx_thread_vars.thread_ref = &raw mut MAIN_THREAD;
    }
}

#[allow(unsafe_op_in_unsafe_fn)]
unsafe fn normal_entry(loader_mode: LoaderMode, exit_config: Option<ExitFn>) -> ! {
    let mut main_thread_handle: svc::Handle = 0;
    let mut heap = util::PointerAndSize::new(ptr::null_mut(), crate::mem::alloc::DEFAULT_HEAP_SIZE);
    let mut hos_version_opt: Option<hbl::Version> = None;
    match loader_mode {
        LoaderMode::Nso(thread_handle) => {
            main_thread_handle = thread_handle;
            set_executable_type(ExecutableType::Nso);
        }
        LoaderMode::Nro(mut abi_entry) => {
            set_executable_type(ExecutableType::Nro);
            unsafe {
                loop {
                    match (*abi_entry).key {
                        hbl::AbiConfigEntryKey::EndOfList => {
                            let loader_info_data = (*abi_entry).value[0] as *mut u8;
                            let loader_info_data_len = (*abi_entry).value[1] as usize;
                            if loader_info_data_len > 0 {
                                let loader_info_slice = core::slice::from_raw_parts(
                                    loader_info_data,
                                    loader_info_data_len,
                                );
                                if let Ok(loader_info) = core::str::from_utf8(loader_info_slice) {
                                    hbl::set_loader_info(loader_info);
                                }
                            }
                            break;
                        }
                        hbl::AbiConfigEntryKey::MainThreadHandle => {
                            main_thread_handle = (*abi_entry).value[0] as svc::Handle;
                        }
                        hbl::AbiConfigEntryKey::NextLoadPath => {
                            // lengths from nx-hbloader:source/main.c
                            // https://github.com/switchbrew/nx-hbloader/blob/cd6a723acbeabffd827a8bdc40563066f5401fb7/source/main.c#L13-L14
                            let next_load_path =
                                core::ptr::with_exposed_provenance_mut::<util::ArrayString<512>>(
                                    (*abi_entry).value[0] as usize,
                                )
                                .as_mut();
                            let next_load_argv =
                                core::ptr::with_exposed_provenance_mut::<util::ArrayString<2048>>(
                                    (*abi_entry).value[1] as usize,
                                )
                                .as_mut();
                            hbl::set_next_load_entry_ptr(next_load_path, next_load_argv);
                        }
                        hbl::AbiConfigEntryKey::OverrideHeap => {
                            heap.address = (*abi_entry).value[0] as *mut u8;
                            heap.size = (*abi_entry).value[1] as usize;
                        }
                        hbl::AbiConfigEntryKey::OverrideService => {
                            // todo!("OverrideService");
                        }
                        hbl::AbiConfigEntryKey::Argv => {
                            // todo!("Argv");
                        }
                        hbl::AbiConfigEntryKey::SyscallAvailableHint => {
                            // todo!("SyscallAvailableHint");
                        }
                        hbl::AbiConfigEntryKey::AppletType => {
                            let applet_type: hbl::AppletType =
                                mem::transmute((*abi_entry).value[0] as u32);
                            hbl::set_applet_type(applet_type);
                        }
                        hbl::AbiConfigEntryKey::ProcessHandle => {
                            let proc_handle = (*abi_entry).value[0] as Handle;
                            hbl::set_process_handle(proc_handle);
                        }
                        hbl::AbiConfigEntryKey::LastLoadResult => {
                            let last_load_rc = ResultCode::new((*abi_entry).value[0] as u32);
                            hbl::set_last_load_result(last_load_rc);
                        }
                        hbl::AbiConfigEntryKey::RandomSeed => {
                            let random_seed = ((*abi_entry).value[0], (*abi_entry).value[1]);
                            hbl::set_random_seed(random_seed);
                        }
                        hbl::AbiConfigEntryKey::UserIdStorage => {
                            // todo!("UserIdStorage");
                        }
                        hbl::AbiConfigEntryKey::HosVersion => {
                            let hos_version_v = (*abi_entry).value[0] as u32;
                            let os_impl_magic = (*abi_entry).value[1];
                            hos_version_opt = Some(hbl::Version::new(hos_version_v, os_impl_magic));
                        }
                        _ => {
                            // TODO: invalid config entries?
                        }
                    }
                    abi_entry = abi_entry.add(1);
                }
            }
        }
    }

    // we need to set up our own ThreadLocalRegion
    unsafe { set_main_thread_tlr(main_thread_handle) };

    // Initialize virtual memory
    vmem::initialize().unwrap();

    // set the exit_fn
    *G_EXIT_FN.lock() = exit_config;

    // Initialize heap and memory allocation
    heap = initialize_heap(heap);
    alloc::initialize(heap);

    let main_thread: thread::Thread = thread::Thread::new_remote("MainThread", main_thread_handle);
    G_MAIN_THREAD.set(Some(main_thread));

    // Initialize version support
    initialize_version(hos_version_opt);

    unsafe { main() };

    // unmount fs devices
    #[cfg(feature = "fs")]
    {
        // clears any globally held fs dev handles
        crate::fs::unmount_all();
        // clears the global fsp_srv session.
        crate::fs::finalize_fspsrv_session();
    }

    #[cfg(feature = "applet")]
    {
        crate::applet::finalize();
    }
    #[cfg(feature = "mii")]
    {
        crate::mii::finalize();
    }

    #[cfg(feature = "la")]
    {
        crate::la::finalize()
    }

    #[cfg(feature = "rand")]
    {
        crate::rand::finalize();
    }

    #[cfg(feature = "socket")]
    {
        crate::socket::finalize();
    }

    // Successful exit by default
    exit(ResultSuccess::make());
}

enum LoaderMode {
    Nso(u32),
    Nro(*const AbiConfigEntry),
}

#[unsafe(no_mangle)]
#[allow(unsafe_op_in_unsafe_fn)]
unsafe extern "C" fn __nx_rrt0_entry(arg0: usize, arg1: usize) -> ! {
    // Since we're using the `b` instruction instead of `bl` in `rrt0.s`, the `lr` register will still have the passed in value.
    // This will be null for NSOs that are directly executed, but has the loader's return pointer for hbl/ovll loaded NROs.
    let lr_raw: usize;
    asm!(
        "mov {}, lr",
        out(reg) lr_raw
    );

    let lr_exit_fn: Option<ExitFn> = match lr_raw {
        0 => None,
        ptr => unsafe { Some(core::mem::transmute::<usize, ExitFn>(ptr)) },
    };

    // We actually want `_start` which is at the start of the .text region, but we don't know if
    // it will be close enough to support lookup via `adr`.
    // Since this function is in `.text` anyway, use QueryMemory SVC to find the actual start
    // This is also a contender to let the compiler decide how to do this by just getting a function pointer (e.g. just get `__nx_rrt0_entry`` as a `fn() - !` type)
    let self_base_address: *mut u8;
    asm!(
        "adr {}, __nx_rrt0_entry",
        out(reg) self_base_address
    );
    let (info, _) = svc::query_memory(self_base_address).unwrap();
    // Use the strict provenance API to convert the usize to a *mut u8 with copied pointer metadata.
    let aslr_base_address = self_base_address.with_addr(info.base_address);

    // assume that the MOD0 structure is at the start of .text
    let mod0 = elf::mod0::Header::from_text_start_addr(aslr_base_address);
    let start_dyn = mod0.get_dyn_start();
    elf::relocate_with_dyn(aslr_base_address, start_dyn);

    mod0.zero_bss_section();

    let eh_hdr_ptr_start = mod0.get_eh_frame_header_start();
    TEXT_BASE_ADDRESS.store(
        self_base_address.expose_provenance(),
        core::sync::atomic::Ordering::Relaxed,
    );
    EH_FRAME_HDR_SECTION.set(eh_hdr_ptr_start);
    let _ = unwinding::custom_eh_frame_finder::set_custom_eh_frame_finder(&EH_FRAME_HDR_SECTION);

    // make sure that the writes are complete before there are any accesses
    core::sync::atomic::compiler_fence(core::sync::atomic::Ordering::SeqCst);

    /*
    Possible entry arguments:
    - NSO/KIP: x0 = 0, x1 = <main-thread-handle>
    - NRO (hbl): x0 = <abi-config-entries-ptr>, x1 = usize::MAX
    - Exception: x0 = <exception-type>, x1 = <stack-top>
    */
    let loader_mode = match (arg0, arg1) {
        (0, main_thread_handle) => LoaderMode::Nso(main_thread_handle as u32),
        (config_pointer, usize::MAX) => {
            LoaderMode::Nro(aslr_base_address.with_addr(config_pointer) as _)
        }
        (exception_type, exception_stack_top) => {
            let exception_type: svc::ExceptionType = mem::transmute(exception_type as u32);
            crate::exception::__nx_exception_dispatch(
                exception_type,
                core::ptr::with_exposed_provenance_mut(exception_stack_top),
            );
        }
    };

    normal_entry(loader_mode, lr_exit_fn);
}

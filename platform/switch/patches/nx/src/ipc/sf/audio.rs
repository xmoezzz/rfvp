use crate::util::ArrayString;
use nx_derive::{Request, Response};

use crate::ipc::sf::{
    self, AppletResourceUserId, CopyHandle, InAutoSelectBuffer, InMapAliasBuffer,
    OutAutoSelectBuffer, OutMapAliasBuffer,
};
use crate::version;

pub mod rc;

pub type AudioInterfaceName = ArrayString<0x100>;

#[derive(Request, Response, Copy, Clone, PartialEq, Eq, Debug)]
#[repr(C)]
pub struct AudioRequestParameters {
    pub sample_rate: u32,
    pub channel_count: u16,
}

#[derive(Request, Response, Copy, Clone, PartialEq, Eq, Debug)]
#[repr(C)]
pub struct AudioResponseParameters {
    pub sample_rate: u32,
    pub channel_count: u32,
    pub sample_format: PcmFormat,
    pub state: u32,
}

#[derive(Request, Response, Copy, Clone, PartialEq, Eq, Debug)]
#[repr(C)]
pub struct AudioBuffer {
    pub _unused_ptr: usize,
    pub sample_buffer: *mut u8,
    pub buffer_capacity: usize,
    pub data_size: usize,
    pub _data_offset: usize,
}

#[derive(Request, Response, Copy, Clone, PartialEq, Eq, Debug)]
#[repr(u32)]
pub enum PcmFormat {
    Invalid = 0,
    Int8 = 1,
    Int16 = 2,
    Int24 = 3,
    Int32 = 4,
    Float = 5,
    Adpcm = 6,
}

#[derive(Request, Response, Copy, Clone, PartialEq, Eq, Debug)]
#[repr(u32)]
pub enum AudioState {
    Started = 0,
    Stopped = 1,
}

#[nx_derive::ipc_trait]
pub trait AudioOutManager {
    #[ipc_rid(0)]
    fn list_audio_outs(&self, names: OutMapAliasBuffer<AudioInterfaceName>) -> u32;
    #[ipc_rid(1)]
    #[return_session]
    fn open_audio_out(
        &self,
        in_name: InMapAliasBuffer<AudioInterfaceName>,
        out_name: OutMapAliasBuffer<AudioInterfaceName>,
        params: AudioRequestParameters,
        aruid: AppletResourceUserId,
        process_handle: sf::CopyHandle,
    ) -> (AudioOut, AudioResponseParameters);
    #[ipc_rid(2)]
    #[version(version::VersionInterval::from(version::Version::new(3, 0, 0)))]
    fn list_audio_outs_auto(&self, names: OutAutoSelectBuffer<AudioInterfaceName>) -> u32;
    #[ipc_rid(3)]
    #[return_session]
    #[version(version::VersionInterval::from(version::Version::new(3, 0, 0)))]
    fn open_audio_out_auto(
        &self,
        in_name: InAutoSelectBuffer<AudioInterfaceName>,
        out_name: OutMapAliasBuffer<AudioInterfaceName>,
        params: AudioRequestParameters,
        aruid: AppletResourceUserId,
        process_handle: sf::CopyHandle,
    ) -> (AudioOut, AudioResponseParameters);
}

#[nx_derive::ipc_trait]
pub trait AudioInManager {
    #[ipc_rid(0)]
    fn list(&self, names: OutMapAliasBuffer<AudioInterfaceName>) -> u32;
    #[ipc_rid(1)]
    #[return_session]
    fn open(
        &self,
        in_name: InMapAliasBuffer<AudioInterfaceName>,
        out_name: OutMapAliasBuffer<AudioInterfaceName>,
        params: AudioRequestParameters,
        aruid: AppletResourceUserId,
        process_handle: sf::CopyHandle,
    ) -> (AudioIn, AudioResponseParameters);
    #[ipc_rid(2)]
    #[version(version::VersionInterval::from(version::Version::new(3, 0, 0)))]
    fn list_auto(&self, names: OutAutoSelectBuffer<AudioInterfaceName>) -> u32;
    #[ipc_rid(3)]
    #[return_session]
    #[version(version::VersionInterval::from(version::Version::new(3, 0, 0)))]
    fn open_auto(
        &self,
        in_name: InAutoSelectBuffer<AudioInterfaceName>,
        out_name: OutMapAliasBuffer<AudioInterfaceName>,
        params: AudioRequestParameters,
        aruid: AppletResourceUserId,
        process_handle: sf::CopyHandle,
    ) -> (AudioIn, AudioResponseParameters);
    #[ipc_rid(4)]
    #[version(version::VersionInterval::from(version::Version::new(4, 0, 0)))]
    fn list_filtered(&self, names: OutAutoSelectBuffer<AudioInterfaceName>) -> u32;
    #[ipc_rid(5)]
    #[return_session]
    #[version(version::VersionInterval::from(version::Version::new(4, 0, 0)))]
    fn open_with_protocol(
        &self,
        in_name: InMapAliasBuffer<AudioInterfaceName>,
        out_name: OutMapAliasBuffer<AudioInterfaceName>,
        params: AudioRequestParameters,
        aruid: AppletResourceUserId,
        process_handle: sf::CopyHandle,
        protocol: u64,
    ) -> (AudioIn, AudioResponseParameters);
}

#[nx_derive::ipc_trait]
#[default_client]
pub trait AudioOut {
    #[ipc_rid(0)]
    fn get_state(&self) -> AudioState;
    #[ipc_rid(1)]
    fn start(&self);
    #[ipc_rid(2)]
    fn stop(&self);
    #[ipc_rid(3)]
    /// # Safety
    ///
    /// The `buffer_ptr` parameter must be a pointer to the `buffer` [`AudioBuffer`]
    unsafe fn append_buffer(&self, buffer: InMapAliasBuffer<AudioBuffer>, buffer_ptr: usize);
    #[ipc_rid(4)]
    fn register_buffer_event(&self) -> CopyHandle;
    #[ipc_rid(5)]
    fn get_released_buffers(&self, buffers: OutMapAliasBuffer<*mut AudioBuffer>) -> u32;
    #[ipc_rid(6)]
    fn contains_buffer(&self, buffer_ptr: usize) -> bool;
    #[ipc_rid(7)]
    #[version(version::VersionInterval::from(version::Version::new(3, 0, 0)))]
    /// # Safety
    ///
    /// The `buffer_ptr` parameter must be a pointer to the `buffer` [`AudioBuffer`].
    unsafe fn append_buffer_auto(&self, buffer: InAutoSelectBuffer<AudioBuffer>, buffer_ptr: usize);
    #[ipc_rid(8)]
    #[version(version::VersionInterval::from(version::Version::new(3, 0, 0)))]
    fn get_released_buffers_auto(&self, buffers: OutAutoSelectBuffer<AudioBuffer>) -> u32;
    #[ipc_rid(9)]
    #[version(version::VersionInterval::from(version::Version::new(4, 0, 0)))]
    fn get_buffer_count(&self) -> u32;
    #[ipc_rid(10)]
    #[version(version::VersionInterval::from(version::Version::new(4, 0, 0)))]
    fn get_played_sample_count(&self) -> u64;
    #[ipc_rid(11)]
    #[version(version::VersionInterval::from(version::Version::new(4, 0, 0)))]
    fn flush_buffers(&self) -> bool;
    #[ipc_rid(12)]
    #[version(version::VersionInterval::from(version::Version::new(6, 0, 0)))]
    fn set_volume(&self, volume: f32);
    #[ipc_rid(13)]
    #[version(version::VersionInterval::from(version::Version::new(6, 0, 0)))]
    fn get_volume(&self) -> u32;
}

#[nx_derive::ipc_trait]
#[default_client]
pub trait AudioIn {
    #[ipc_rid(0)]
    fn get_state(&self) -> AudioState;
    #[ipc_rid(1)]
    fn start(&self);
    #[ipc_rid(2)]
    fn stop(&self);
    #[ipc_rid(3)]
    /// # Safety
    ///
    /// The `buffer_ptr` parameter must be a pointer to the `buffer` [`AudioBuffer`]
    unsafe fn append_buffer(&self, buffer: InMapAliasBuffer<AudioBuffer>, buffer_ptr: usize);
    #[ipc_rid(4)]
    fn register_buffer_event(&self) -> CopyHandle;
    #[ipc_rid(5)]
    fn get_released_buffers(&self, buffers: OutMapAliasBuffer<*mut AudioBuffer>) -> u32;
    #[ipc_rid(6)]
    fn contains_buffer(&self, buffer_ptr: usize) -> bool;
    #[ipc_rid(8)]
    #[version(version::VersionInterval::from(version::Version::new(3, 0, 0)))]
    /// # Safety
    ///
    /// The `buffer_ptr` parameter must be a pointer to the `buffer` [`AudioBuffer`]
    unsafe fn append_buffer_auto(&self, buffer: InAutoSelectBuffer<AudioBuffer>, buffer_ptr: usize);
    #[ipc_rid(9)]
    #[version(version::VersionInterval::from(version::Version::new(3, 0, 0)))]
    fn get_released_buffers_auto(&self, buffers: OutAutoSelectBuffer<*mut AudioBuffer>) -> u32;
    #[ipc_rid(11)]
    #[version(version::VersionInterval::from(version::Version::new(4, 0, 0)))]
    fn get_buffer_count(&self) -> u32;
    #[ipc_rid(12)]
    #[version(version::VersionInterval::from(version::Version::new(4, 0, 0)))]
    fn set_device_gain(&mut self, gain: f32);
    #[ipc_rid(13)]
    #[version(version::VersionInterval::from(version::Version::new(4, 0, 0)))]
    fn get_device_gain(&self) -> f32;
    #[ipc_rid(14)]
    #[version(version::VersionInterval::from(version::Version::new(6, 0, 0)))]
    fn flush_buffers(&self) -> bool;
}

pub mod audio;
pub mod clock;
pub mod error;
pub mod event;
pub mod fs;
pub mod host;
pub mod render;
pub mod soft_raster;
mod srgb_tables;
#[cfg(all(
    any(not(feature = "no_std"), test),
    not(target_os = "uefi"),
    not(target_arch = "wasm32")
))]
pub mod std_fs;
#[cfg(all(not(feature = "no_std"), target_os = "uefi", feature = "rfvp-os"))]
pub mod uefi_fs;

pub use audio::{
    AudioParams, AudioSampleFormat, AudioSlotKind, AudioStreamDesc, AudioStreamId,
    EncodedAudioKind, RfvpAudio, SoftAudioConfig, SoftAudioMixer, SoftAudioVorbis,
    BGM_LOGICAL_SLOT_COUNT, SE_LOGICAL_SLOT_COUNT, SE_STREAM_ID_BASE,
};
pub use clock::{RfvpClock, WallTime};
pub use error::{RfvpError, RfvpResult};
pub use event::{InputModifiers, KeyCode, PointerButton, RfvpEvent};
pub use soft_raster::SoftRaster;
pub use fs::{
    RfvpAssetPath, RfvpFile, RfvpFileInfo, RfvpFileKind, RfvpFileSystem, RfvpStorage,
};
#[cfg(feature = "no_std")]
pub use host::{FatalErrorCallback, FatalErrorCode, PlatformCallbacks};
pub use host::{RfvpHost, RfvpLogLevel};
pub use render::{
    BlendMode, ColorRgba, CommandBlendMode, DrawGlyphCmd, DrawImageCmd, DrawSolidCommand,
    DrawSpriteCommand, HitProxy, HitProxyTable, PixelFormat, PortableTextureDesc, PrimId, RectI16,
    RectI32, RectU16, RenderBackend, RenderCommand, RenderFrame, RenderTargetId, ResourceId,
    RfvpRenderer, Rgba8, TextureBackend, TextureDesc, TextureFilter, TextureFormat, TextureHandle,
    TextureId, TextureRect, Vertex2D,
};

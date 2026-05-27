pub mod audio;
pub mod clock;
pub mod error;
pub mod event;
pub mod fs;
pub mod host;
pub mod render;

pub use audio::{
    AudioParams, AudioSampleFormat, AudioSlotKind, AudioStreamDesc, AudioStreamId,
    EncodedAudioKind, RfvpAudio, SoftAudioConfig, SoftAudioMixer, SoftAudioVorbis,
    BGM_LOGICAL_SLOT_COUNT, SE_LOGICAL_SLOT_COUNT, SE_STREAM_ID_BASE,
};
pub use clock::RfvpClock;
pub use error::{RfvpError, RfvpResult};
pub use event::{InputModifiers, KeyCode, PointerButton, RfvpEvent};
pub use fs::{RfvpAssetPath, RfvpFile, RfvpFileInfo, RfvpFileKind, RfvpFileSystem};
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

use alloc::format;
use alloc::string::String;
use alloc::vec::Vec;

use crate::host_api::{
    AudioParams, AudioStreamDesc, AudioStreamId, DrawSolidCommand, DrawSpriteCommand, RfvpAudio,
    RfvpError, RfvpHost, RfvpLogLevel, RfvpRenderer, RfvpResult, TextureDesc, TextureId,
    TextureRect,
};

use super::values::Variant;
use super::subsystem::PortableSubsystem;
use super::vm::{ThreadRequest, VmError, VmResult};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NativeCallSite {
    pub thread_id: u32,
    pub pc: usize,
    pub syscall_id: u16,
    pub syscall_name: String,
}

pub enum NativeSyscall {
    Immediate(Variant),
    ThreadStart { id: u32, address: u32 },
    ThreadWait { ms: u32 },
    ThreadSleep { ms: u32 },
    ThreadRaise { ms: u32 },
    ThreadNext,
    ThreadExit { id: Option<u32> },
    RenderSprite(DrawSpriteCommand),
    RenderSolid(DrawSolidCommand),
    CreateTexture { id: TextureId, desc: TextureDesc, pixels: Vec<u8> },
    UpdateTexture { id: TextureId, rect: TextureRect, pixels: Vec<u8> },
    DestroyTexture { id: TextureId },
    AudioCreateStream { id: AudioStreamId, desc: AudioStreamDesc },
    AudioPlay { id: AudioStreamId, params: AudioParams },
    AudioStop { id: AudioStreamId, fade_ms: u32 },
    Log { level: RfvpLogLevel, message: String },
    Unsupported { reason: String },
}

pub struct PortableNativeBridge<'a, H: RfvpHost> {
    pub(crate) host: &'a mut H,
    subsystem: &'a mut PortableSubsystem,
    requests: Vec<ThreadRequest>,
}

impl<'a, H: RfvpHost> PortableNativeBridge<'a, H> {
    pub fn new(host: &'a mut H, subsystem: &'a mut PortableSubsystem) -> Self {
        Self {
            host,
            subsystem,
            requests: Vec::new(),
        }
    }

    pub fn take_requests(&mut self) -> Vec<ThreadRequest> {
        core::mem::take(&mut self.requests)
    }

    pub fn syscall(
        &mut self,
        call_site: NativeCallSite,
        args: Vec<Variant>,
    ) -> VmResult<Variant> {
        match self.decode_syscall(&call_site, args)? {
            NativeSyscall::Immediate(value) => Ok(value),
            NativeSyscall::ThreadStart { id, address } => {
                self.requests.push(ThreadRequest::Start(id, address));
                Ok(Variant::Nil)
            }
            NativeSyscall::ThreadWait { ms } => {
                self.requests.push(ThreadRequest::Wait(ms));
                Ok(Variant::Nil)
            }
            NativeSyscall::ThreadSleep { ms } => {
                self.requests.push(ThreadRequest::Sleep(ms));
                Ok(Variant::Nil)
            }
            NativeSyscall::ThreadRaise { ms } => {
                self.requests.push(ThreadRequest::Raise(ms));
                Ok(Variant::Nil)
            }
            NativeSyscall::ThreadNext => {
                self.requests.push(ThreadRequest::Next);
                Ok(Variant::Nil)
            }
            NativeSyscall::ThreadExit { id } => {
                self.requests.push(ThreadRequest::Exit(id));
                Ok(Variant::Nil)
            }
            NativeSyscall::RenderSprite(command) => {
                self.host.renderer().draw_sprite(&command).map_err(host_error)?;
                Ok(Variant::Nil)
            }
            NativeSyscall::RenderSolid(command) => {
                self.host.renderer().draw_solid(&command).map_err(host_error)?;
                Ok(Variant::Nil)
            }
            NativeSyscall::CreateTexture { id, desc, pixels } => {
                self.host
                    .renderer()
                    .create_texture(id, desc, Some(&pixels))
                    .map_err(host_error)?;
                Ok(Variant::Nil)
            }
            NativeSyscall::UpdateTexture { id, rect, pixels } => {
                self.host
                    .renderer()
                    .update_texture(id, rect, &pixels)
                    .map_err(host_error)?;
                Ok(Variant::Nil)
            }
            NativeSyscall::DestroyTexture { id } => {
                self.host.renderer().destroy_texture(id);
                Ok(Variant::Nil)
            }
            NativeSyscall::AudioCreateStream { id, desc } => {
                self.host.audio().create_stream(id, desc).map_err(host_error)?;
                Ok(Variant::Nil)
            }
            NativeSyscall::AudioPlay { id, params } => {
                self.host.audio().play(id, params).map_err(host_error)?;
                Ok(Variant::Nil)
            }
            NativeSyscall::AudioStop { id, fade_ms } => {
                self.host.audio().stop(id, fade_ms).map_err(host_error)?;
                Ok(Variant::Nil)
            }
            NativeSyscall::Log { level, message } => {
                self.host.log(level, &message);
                Ok(Variant::Nil)
            }
            NativeSyscall::Unsupported { reason } => Err(VmError::missing_native(
                call_site.syscall_name,
                call_site.syscall_id,
                call_site.pc,
                call_site.thread_id,
                reason,
            )),
        }
    }

    fn decode_syscall(
        &mut self,
        call_site: &NativeCallSite,
        args: Vec<Variant>,
    ) -> VmResult<NativeSyscall> {
        if let Some(result) = self.subsystem.syscall(self.host, &call_site.syscall_name, &args) {
            return result.map(|value| NativeSyscall::Immediate(value)).map_err(host_error);
        }
        match call_site.syscall_name.as_str() {
            "ThreadStart" => Ok(NativeSyscall::ThreadStart {
                id: arg_i32(&args, 0, call_site)? as u32,
                address: arg_i32(&args, 1, call_site)? as u32,
            }),
            "ThreadWait" => Ok(NativeSyscall::ThreadWait {
                ms: arg_i32(&args, 0, call_site)? as u32,
            }),
            "ThreadSleep" => Ok(NativeSyscall::ThreadSleep {
                ms: arg_i32(&args, 0, call_site)? as u32,
            }),
            "ThreadRaise" => Ok(NativeSyscall::ThreadRaise {
                ms: arg_i32(&args, 0, call_site)? as u32,
            }),
            "ThreadNext" => Ok(NativeSyscall::ThreadNext),
            "ThreadExit" => {
                let id = args.first().and_then(Variant::as_int).map(|v| v as u32);
                Ok(NativeSyscall::ThreadExit { id })
            }
            "Debmess" => Ok(NativeSyscall::Log {
                level: RfvpLogLevel::Debug,
                message: format!("{:?}", args),
            }),
            other => Ok(NativeSyscall::Unsupported {
                reason: format!(
                    "no no_std native bridge implementation for syscall `{}` with {} argument(s)",
                    other,
                    args.len()
                ),
            }),
        }
    }
}

fn arg_i32(args: &[Variant], index: usize, call_site: &NativeCallSite) -> VmResult<i32> {
    args.get(index)
        .and_then(Variant::as_int)
        .ok_or_else(|| {
            VmError::missing_native(
                call_site.syscall_name.clone(),
                call_site.syscall_id,
                call_site.pc,
                call_site.thread_id,
                format!("argument {} is not an integer", index),
            )
        })
}

fn host_error(err: RfvpError) -> VmError {
    VmError::Host(err)
}

pub fn map_vm_error(err: VmError) -> RfvpError {
    match err {
        VmError::Host(err) => err,
        VmError::InvalidData { .. } => RfvpError::InvalidData,
        VmError::UnsupportedNative { .. } => RfvpError::Unsupported,
        VmError::Runtime { .. } => RfvpError::Backend,
    }
}

pub fn render_vm_error<H: RfvpHost>(host: &mut H, err: &VmError) -> RfvpResult<()> {
    let message = err.to_message();
    host.log(RfvpLogLevel::Error, &message);
    Err(map_vm_error(err.clone()))
}

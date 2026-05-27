use alloc::string::String;
use alloc::vec::Vec;

use crate::host_api::{
    BlendMode, ColorRgba, DrawSolidCommand, RectI32, RfvpError, RfvpFileInfo, RfvpHost,
    RfvpLogLevel, RfvpResult, RfvpEvent, RfvpFile, RfvpFileSystem, RfvpRenderer,
};

use super::native_bridge::{map_vm_error, PortableNativeBridge};
use super::parser::{Nls, Parser};
use super::subsystem::PortableSubsystem;
use super::vm::PortableVm;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PortableTickReport {
    pub main_thread_exited: bool,
}

pub struct PortableRuntime {
    parser: Parser,
    vm: PortableVm,
    subsystem: PortableSubsystem,
    title: String,
    screen_size: (u32, u32),
    last_error: Option<String>,
}

impl PortableRuntime {
    pub fn boot_from_hcb_bytes(buffer: Vec<u8>, nls: Nls) -> RfvpResult<Self> {
        let parser = Parser::from_bytes(buffer, nls).map_err(map_vm_error)?;
        let title = parser.get_title();
        let screen_size = parser.get_screen_size();
        let mut vm = PortableVm::new(
            parser.non_volatile_global_count,
            parser.volatile_global_count,
        );
        vm.start_main(parser.get_entry_point());
        Ok(Self {
            parser,
            vm,
            subsystem: PortableSubsystem::new(),
            title,
            screen_size,
            last_error: None,
        })
    }

    pub fn title(&self) -> &str {
        &self.title
    }

    pub fn screen_size(&self) -> (u32, u32) {
        self.screen_size
    }

    pub fn last_error(&self) -> Option<&str> {
        self.last_error.as_deref()
    }

    pub fn tick<H: RfvpHost>(
        &mut self,
        host: &mut H,
        frame_time_ms: u64,
    ) -> RfvpResult<PortableTickReport> {
        self.subsystem.begin_frame();
        let mut bridge = PortableNativeBridge::new(host, &mut self.subsystem);
        match self.vm.tick(&mut self.parser, &mut bridge, frame_time_ms) {
            Ok(()) => {
                self.last_error = None;
                Ok(PortableTickReport {
                    main_thread_exited: self.vm.thread_break(),
                })
            }
            Err(err) => {
                let message = err.to_message();
                self.last_error = Some(message.clone());
                bridge.host.log(RfvpLogLevel::Error, &message);
                Err(map_vm_error(err))
            }
        }
    }

    pub fn render_frame<H: RfvpHost>(
        &mut self,
        host: &mut H,
        frame_index: u64,
    ) -> RfvpResult<()> {
        if self.subsystem_has_render_state() {
            let (width, height) = self.screen_size;
            return self.subsystem.render(host, width, height);
        }
        let (width, height) = self.screen_size;
        host.renderer()
            .begin_frame(width, height, Some(ColorRgba::BLACK))?;
        let scan_width = ((frame_index as u32 % width.max(1)).max(1)) as i32;
        host.renderer().draw_solid(&DrawSolidCommand {
            rect: RectI32 {
                x: 0,
                y: 0,
                width: scan_width,
                height: 2,
            },
            color: ColorRgba {
                r: 0.1,
                g: 0.6,
                b: 0.3,
                a: 1.0,
            },
            blend: BlendMode::Opaque,
            scissor: None,
        })?;
        host.renderer().end_frame()?;
        host.renderer().present()
    }

    pub fn handle_event(&mut self, event: RfvpEvent) {
        self.subsystem.handle_event(event);
    }

    fn subsystem_has_render_state(&self) -> bool {
        true
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LoadedHcb {
    pub path: String,
    pub bytes: Vec<u8>,
    pub info: RfvpFileInfo,
}

pub fn find_and_load_hcb<H: RfvpHost>(
    host: &mut H,
    root: &str,
    extension: &str,
    max_hcb_bytes: usize,
    max_manifest_entries: usize,
) -> RfvpResult<(LoadedHcb, Vec<(String, RfvpFileInfo)>)> {
    if root.is_empty() || extension.is_empty() {
        return Err(RfvpError::InvalidArgument);
    }
    let mut found = Vec::new();
    {
        let visitor = &mut |path: &str, info: RfvpFileInfo| -> RfvpResult<()> {
            if found.len() >= max_manifest_entries {
                return Err(RfvpError::CapacityExceeded);
            }
            found.push((String::from(path), info));
            Ok(())
        };
        host.fs().enumerate_by_extension(root, extension, visitor)?;
    }
    let Some((path, info)) = found.first().cloned() else {
        return Err(RfvpError::NotFound);
    };
    let mut file = host.fs().open(&path)?;
    let bytes = file.read_to_vec(max_hcb_bytes)?;
    if bytes.is_empty() {
        return Err(RfvpError::InvalidData);
    }
    let manifest = found.into_iter().skip(1).collect();
    Ok((LoadedHcb { path, bytes, info }, manifest))
}

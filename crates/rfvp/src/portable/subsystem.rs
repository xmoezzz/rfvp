use alloc::string::String;
use alloc::vec::Vec;

use crate::host_api::{
    AudioParams, AudioSampleFormat, AudioStreamDesc, AudioStreamId, BlendMode, ColorRgba,
    DrawSolidCommand, KeyCode, PointerButton, RectI32, RfvpAudio, RfvpEvent, RfvpFile,
    RfvpFileSystem, RfvpHost, RfvpRenderer, RfvpResult,
};

use super::values::{Table, Variant};

const PRIM_COUNT: usize = 4096;
const AUDIO_SLOT_COUNT: usize = 16;

#[derive(Debug, Clone)]
pub struct PortableSubsystem {
    prims: Vec<Prim>,
    resources: Vec<ResourceEntry>,
    audio_slots: Vec<AudioSlot>,
    input: InputState,
    root_prim: u16,
    window_mode: i32,
    exit_mode: i32,
    master_volume: i32,
}

#[derive(Debug, Clone)]
pub struct Prim {
    pub id: u16,
    pub kind: PrimKind,
    pub parent: Option<u16>,
    pub x: i32,
    pub y: i32,
    pub width: i32,
    pub height: i32,
    pub z: i32,
    pub alpha: u8,
    pub draw: bool,
    pub blend: BlendMode,
    pub color: ColorRgba,
    pub resource_id: Option<u16>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PrimKind {
    None,
    Group,
    Sprite,
    Tile,
    Text,
    Snow,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResourceEntry {
    pub id: u16,
    pub path: String,
    pub bytes: Vec<u8>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct AudioSlot {
    pub id: u32,
    pub path: Option<String>,
    pub volume: f32,
    pub pan: f32,
    pub repeat: bool,
    pub playing: bool,
    pub silent: bool,
    pub sound_type: i32,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct InputState {
    cursor_x: i32,
    cursor_y: i32,
    cursor_in: bool,
    down_bits: u32,
    state_bits: u32,
    up_bits: u32,
    repeat_bits: u32,
    wheel: i32,
    events: Vec<InputEvent>,
    click_mode: u32,
    control_masked: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InputEvent {
    keycode: i32,
    x: i32,
    y: i32,
}

impl PortableSubsystem {
    pub fn new() -> Self {
        let mut prims = Vec::with_capacity(PRIM_COUNT);
        for id in 0..PRIM_COUNT {
            prims.push(Prim::new(id as u16));
        }
        let mut audio_slots = Vec::with_capacity(AUDIO_SLOT_COUNT);
        for id in 0..AUDIO_SLOT_COUNT {
            audio_slots.push(AudioSlot {
                id: id as u32,
                path: None,
                volume: 1.0,
                pan: 0.0,
                repeat: false,
                playing: false,
                silent: false,
                sound_type: 0,
            });
        }
        Self {
            prims,
            resources: Vec::new(),
            audio_slots,
            input: InputState::default(),
            root_prim: 0,
            window_mode: 0,
            exit_mode: 0,
            master_volume: 100,
        }
    }

    pub fn handle_event(&mut self, event: RfvpEvent) {
        self.input.handle_event(event);
    }

    pub fn begin_frame(&mut self) {
        self.input.down_bits = 0;
        self.input.up_bits = 0;
        self.input.repeat_bits = 0;
        self.input.wheel = 0;
    }

    pub fn render<H: RfvpHost>(&self, host: &mut H, width: u32, height: u32) -> RfvpResult<()> {
        host.renderer()
            .begin_frame(width, height, Some(ColorRgba::BLACK))?;
        for prim in self.prims.iter().filter(|p| p.draw && p.kind != PrimKind::None) {
            if prim.width <= 0 || prim.height <= 0 {
                continue;
            }
            let mut color = prim.color;
            color.a *= prim.alpha as f32 / 255.0;
            host.renderer().draw_solid(&DrawSolidCommand {
                rect: RectI32 {
                    x: prim.x,
                    y: prim.y,
                    width: prim.width,
                    height: prim.height,
                },
                color,
                blend: prim.blend,
                scissor: None,
            })?;
        }
        host.renderer().end_frame()?;
        host.renderer().present()
    }

    pub fn syscall<H: RfvpHost>(
        &mut self,
        host: &mut H,
        name: &str,
        args: &[Variant],
    ) -> Option<RfvpResult<Variant>> {
        let result = match name {
            "WindowMode" => {
                if let Some(v) = arg_int(args, 0) {
                    self.window_mode = v;
                }
                Ok(Variant::Nil)
            }
            "ExitMode" => {
                if let Some(v) = arg_int(args, 0) {
                    self.exit_mode = v;
                }
                Ok(Variant::Nil)
            }
            "GraphLoad" => self.graph_load(host, args),
            "GraphRGB" => {
                if let Some(id) = arg_int(args, 0) {
                    let (r, g, b) = (
                        arg_int(args, 1).unwrap_or(100),
                        arg_int(args, 2).unwrap_or(100),
                        arg_int(args, 3).unwrap_or(100),
                    );
                    self.set_resource_color(id as u16, r, g, b);
                }
                Ok(Variant::Nil)
            }
            "PrimExitGroup" => {
                if let Some(id) = checked_prim_id(args, 0) {
                    self.root_prim = id;
                }
                Ok(Variant::Nil)
            }
            "PrimSetNull" => {
                if let Some(id) = checked_prim_id(args, 0) {
                    self.prim_mut(id).reset();
                }
                Ok(Variant::Nil)
            }
            "PrimSetSprt" | "PrimSetTile" | "PrimSetText" => {
                if let Some(id) = checked_prim_id(args, 0) {
                    let resource = arg_int(args, 1).map(|v| v as u16);
                    let prim = self.prim_mut(id);
                    prim.kind = match name {
                        "PrimSetTile" => PrimKind::Tile,
                        "PrimSetText" => PrimKind::Text,
                        _ => PrimKind::Sprite,
                    };
                    prim.resource_id = resource;
                    prim.draw = true;
                }
                Ok(Variant::Nil)
            }
            "PrimSetXY" => {
                if let Some(id) = checked_prim_id(args, 0) {
                    let prim = self.prim_mut(id);
                    if let Some(x) = arg_int(args, 1) {
                        prim.x = x;
                    }
                    if let Some(y) = arg_int(args, 2) {
                        prim.y = y;
                    }
                }
                Ok(Variant::Nil)
            }
            "PrimSetWH" => {
                if let Some(id) = checked_prim_id(args, 0) {
                    let prim = self.prim_mut(id);
                    if let Some(w) = arg_int(args, 1) {
                        prim.width = w;
                    }
                    if let Some(h) = arg_int(args, 2) {
                        prim.height = h;
                    }
                }
                Ok(Variant::Nil)
            }
            "PrimSetAlpha" => {
                if let Some(id) = checked_prim_id(args, 0) {
                    if let Some(alpha) = arg_int(args, 1) {
                        self.prim_mut(id).alpha = alpha.clamp(0, 255) as u8;
                    }
                }
                Ok(Variant::Nil)
            }
            "PrimSetDraw" => {
                if let Some(id) = checked_prim_id(args, 0) {
                    self.prim_mut(id).draw = arg_int(args, 1).unwrap_or(0) != 0;
                }
                Ok(Variant::Nil)
            }
            "PrimSetBlend" => {
                if let Some(id) = checked_prim_id(args, 0) {
                    self.prim_mut(id).blend = if arg_int(args, 1).unwrap_or(0) == 0 {
                        BlendMode::Opaque
                    } else {
                        BlendMode::Alpha
                    };
                }
                Ok(Variant::Nil)
            }
            "PrimSetZ" => {
                if let Some(id) = checked_prim_id(args, 0) {
                    self.prim_mut(id).z = arg_int(args, 1).unwrap_or(0);
                }
                Ok(Variant::Nil)
            }
            "PrimGroupIn" | "PrimGroupMove" => {
                if let (Some(id), Some(parent)) = (checked_prim_id(args, 0), checked_prim_id(args, 1)) {
                    self.prim_mut(id).parent = Some(parent);
                }
                Ok(Variant::Nil)
            }
            "PrimGroupOut" => {
                if let Some(id) = checked_prim_id(args, 0) {
                    self.prim_mut(id).parent = None;
                }
                Ok(Variant::Nil)
            }
            "PrimHit" => Ok(Variant::Nil),
            "SoundLoad" | "AudioLoad" => self.audio_load(host, args),
            "SoundPlay" | "AudioPlay" => self.audio_play(host, args),
            "SoundStop" | "AudioStop" => self.audio_stop(host, args),
            "SoundVol" | "AudioVol" => {
                if let Some(slot) = checked_audio_slot(args, 0) {
                    let vol = arg_int(args, 1).unwrap_or(100).clamp(0, 100) as f32 / 100.0;
                    self.audio_slots[slot].volume = vol;
                    let id = AudioStreamId(slot as u32);
                    let params = self.audio_params(slot);
                    if let Err(err) = host.audio().set_params(id, params) {
                        return Some(Err(err));
                    }
                }
                Ok(Variant::Nil)
            }
            "SoundMasterVol" => {
                self.master_volume = arg_int(args, 0).unwrap_or(self.master_volume).clamp(0, 100);
                Ok(Variant::Nil)
            }
            "SoundSilentOn" | "AudioSilentOn" => {
                if let Some(slot) = checked_audio_slot(args, 0) {
                    self.audio_slots[slot].silent = true;
                    if let Err(err) = host.audio().stop(AudioStreamId(slot as u32), 0) {
                        return Some(Err(err));
                    }
                }
                Ok(Variant::Nil)
            }
            "SoundType" | "AudioType" => {
                if let Some(slot) = checked_audio_slot(args, 0) {
                    self.audio_slots[slot].sound_type = arg_int(args, 1).unwrap_or(0);
                }
                Ok(Variant::Nil)
            }
            "SoundTypeVol" => Ok(Variant::Nil),
            "SoundPan" => {
                if let Some(slot) = checked_audio_slot(args, 0) {
                    self.audio_slots[slot].pan =
                        (arg_int(args, 1).unwrap_or(50).clamp(0, 100) as f32 - 50.0) / 50.0;
                    let params = self.audio_params(slot);
                    if let Err(err) = host.audio().set_params(AudioStreamId(slot as u32), params) {
                        return Some(Err(err));
                    }
                }
                Ok(Variant::Nil)
            }
            "AudioState" => {
                let playing = checked_audio_slot(args, 0)
                    .map(|slot| self.audio_slots[slot].playing)
                    .unwrap_or(false);
                Ok(if playing { Variant::True } else { Variant::Nil })
            }
            "InputFlash" => {
                self.input.begin_poll();
                Ok(Variant::Nil)
            }
            "InputGetCursIn" => Ok(if self.input.cursor_in { Variant::True } else { Variant::Nil }),
            "InputGetCursX" => Ok(Variant::Int(self.input.cursor_x)),
            "InputGetCursY" => Ok(Variant::Int(self.input.cursor_y)),
            "InputGetDown" => Ok(Variant::Int(self.input.down_bits as i32)),
            "InputGetRepeat" => Ok(Variant::Int(self.input.repeat_bits as i32)),
            "InputGetState" => Ok(Variant::Int(self.input.state_bits as i32)),
            "InputGetUp" => Ok(Variant::Int(self.input.up_bits as i32)),
            "InputGetWheel" => Ok(Variant::Int(self.input.wheel)),
            "InputGetEvent" => Ok(self.input.pop_event_variant()),
            "InputSetClick" => {
                self.input.click_mode = arg_int(args, 0).unwrap_or(0).max(0) as u32;
                Ok(Variant::Nil)
            }
            "ControlPulse" => Ok(Variant::Nil),
            "ControlMask" => {
                self.input.control_masked = args.first().map(Variant::is_nil).unwrap_or(true);
                Ok(Variant::Nil)
            }
            _ => return None,
        };
        Some(result)
    }

    fn graph_load<H: RfvpHost>(&mut self, host: &mut H, args: &[Variant]) -> RfvpResult<Variant> {
        let Some(id) = arg_int(args, 0) else {
            return Ok(Variant::Nil);
        };
        let Some(path) = arg_string(args, 1) else {
            self.remove_resource(id as u16);
            return Ok(Variant::Nil);
        };
        let bytes = read_resource(host, &path)?;
        self.insert_resource(ResourceEntry {
            id: id as u16,
            path,
            bytes,
        });
        Ok(Variant::Nil)
    }

    fn audio_load<H: RfvpHost>(&mut self, host: &mut H, args: &[Variant]) -> RfvpResult<Variant> {
        let Some(slot) = checked_audio_slot(args, 0) else {
            return Ok(Variant::Nil);
        };
        let Some(path) = arg_string(args, 1) else {
            self.audio_slots[slot].path = None;
            host.audio().destroy_stream(AudioStreamId(slot as u32));
            return Ok(Variant::Nil);
        };
        let bytes = read_resource(host, &path)?;
        self.audio_slots[slot].path = Some(path);
        host.audio().destroy_stream(AudioStreamId(slot as u32));
        host.audio().create_stream(
            AudioStreamId(slot as u32),
            AudioStreamDesc {
                sample_rate: 44_100,
                channels: 2,
                sample_format: AudioSampleFormat::I16,
            },
        )?;
        host.audio().submit_i16(AudioStreamId(slot as u32), &[])?;
        drop(bytes);
        Ok(Variant::Nil)
    }

    fn audio_play<H: RfvpHost>(&mut self, host: &mut H, args: &[Variant]) -> RfvpResult<Variant> {
        let Some(slot) = checked_audio_slot(args, 0) else {
            return Ok(Variant::Nil);
        };
        self.audio_slots[slot].repeat = args.get(1).map(Variant::canbe_true).unwrap_or(false);
        if self.audio_slots[slot].silent {
            return Ok(Variant::Nil);
        }
        self.audio_slots[slot].playing = true;
        host.audio()
            .play(AudioStreamId(slot as u32), self.audio_params(slot))?;
        Ok(Variant::Nil)
    }

    fn audio_stop<H: RfvpHost>(&mut self, host: &mut H, args: &[Variant]) -> RfvpResult<Variant> {
        let Some(slot) = checked_audio_slot(args, 0) else {
            return Ok(Variant::Nil);
        };
        let fade_ms = arg_int(args, 1).unwrap_or(0).clamp(0, 300_000) as u32;
        self.audio_slots[slot].playing = false;
        host.audio().stop(AudioStreamId(slot as u32), fade_ms)?;
        Ok(Variant::Nil)
    }

    fn audio_params(&self, slot: usize) -> AudioParams {
        let slot = &self.audio_slots[slot];
        AudioParams {
            volume: slot.volume * (self.master_volume as f32 / 100.0),
            pan: slot.pan,
            repeat: slot.repeat,
        }
    }

    fn prim_mut(&mut self, id: u16) -> &mut Prim {
        &mut self.prims[id as usize]
    }

    fn insert_resource(&mut self, resource: ResourceEntry) {
        self.remove_resource(resource.id);
        self.resources.push(resource);
    }

    fn remove_resource(&mut self, id: u16) {
        self.resources.retain(|entry| entry.id != id);
    }

    fn set_resource_color(&mut self, id: u16, r: i32, g: i32, b: i32) {
        let color = ColorRgba {
            r: r.clamp(0, 200) as f32 / 100.0,
            g: g.clamp(0, 200) as f32 / 100.0,
            b: b.clamp(0, 200) as f32 / 100.0,
            a: 1.0,
        };
        for prim in self.prims.iter_mut().filter(|p| p.resource_id == Some(id)) {
            prim.color = color;
        }
    }
}

impl Default for PortableSubsystem {
    fn default() -> Self {
        Self::new()
    }
}

impl Prim {
    fn new(id: u16) -> Self {
        Self {
            id,
            kind: PrimKind::None,
            parent: None,
            x: 0,
            y: 0,
            width: 64,
            height: 64,
            z: 0,
            alpha: 255,
            draw: false,
            blend: BlendMode::Alpha,
            color: ColorRgba {
                r: 1.0,
                g: 1.0,
                b: 1.0,
                a: 1.0,
            },
            resource_id: None,
        }
    }

    fn reset(&mut self) {
        *self = Self::new(self.id);
    }
}

impl InputState {
    fn begin_poll(&mut self) {
        self.down_bits = 0;
        self.up_bits = 0;
        self.repeat_bits = 0;
        self.wheel = 0;
    }

    fn handle_event(&mut self, event: RfvpEvent) {
        match event {
            RfvpEvent::PointerMove { x, y, in_screen } => {
                self.cursor_x = x;
                self.cursor_y = y;
                self.cursor_in = in_screen;
            }
            RfvpEvent::PointerDown { button, x, y } => {
                self.cursor_x = x;
                self.cursor_y = y;
                let bit = pointer_bit(button);
                self.down_bits |= bit;
                self.state_bits |= bit;
                self.events.push(InputEvent {
                    keycode: bit.trailing_zeros() as i32,
                    x,
                    y,
                });
            }
            RfvpEvent::PointerUp { button, x, y } => {
                self.cursor_x = x;
                self.cursor_y = y;
                let bit = pointer_bit(button);
                self.up_bits |= bit;
                self.state_bits &= !bit;
            }
            RfvpEvent::KeyDown { key, repeat, .. } => {
                let bit = key_bit(key);
                self.down_bits |= bit;
                self.state_bits |= bit;
                if repeat {
                    self.repeat_bits |= bit;
                }
                self.events.push(InputEvent {
                    keycode: bit.trailing_zeros() as i32,
                    x: self.cursor_x,
                    y: self.cursor_y,
                });
            }
            RfvpEvent::KeyUp { key, .. } => {
                let bit = key_bit(key);
                self.up_bits |= bit;
                self.state_bits &= !bit;
            }
            RfvpEvent::Wheel { delta_y, .. } => {
                self.wheel = self.wheel.saturating_add(delta_y);
            }
            _ => {}
        }
    }

    fn pop_event_variant(&mut self) -> Variant {
        let Some(event) = self.events.pop() else {
            return Variant::Nil;
        };
        let mut table = Table::new();
        table.insert(0, Variant::Int(event.keycode));
        table.insert(1, Variant::Int(event.x));
        table.insert(2, Variant::Int(event.y));
        Variant::Table(table)
    }
}

fn arg_int(args: &[Variant], index: usize) -> Option<i32> {
    args.get(index).and_then(Variant::as_int)
}

fn arg_string(args: &[Variant], index: usize) -> Option<String> {
    match args.get(index) {
        Some(Variant::String(s)) | Some(Variant::ConstString(s, _)) => Some(s.clone()),
        _ => None,
    }
}

fn checked_prim_id(args: &[Variant], index: usize) -> Option<u16> {
    let id = arg_int(args, index)?;
    (0..PRIM_COUNT as i32).contains(&id).then_some(id as u16)
}

fn checked_audio_slot(args: &[Variant], index: usize) -> Option<usize> {
    let id = arg_int(args, index)?;
    (0..AUDIO_SLOT_COUNT as i32).contains(&id).then_some(id as usize)
}

fn read_resource<H: RfvpHost>(host: &mut H, path: &str) -> RfvpResult<Vec<u8>> {
    let mut file = host.fs().open(path)?;
    file.read_to_vec(64 * 1024 * 1024)
}

fn key_bit(key: KeyCode) -> u32 {
    let code = match key {
        KeyCode::Shift => 0,
        KeyCode::Control => 1,
        KeyCode::Escape => 6,
        KeyCode::Return => 7,
        KeyCode::Space => 8,
        KeyCode::Up => 9,
        KeyCode::Down => 10,
        KeyCode::Left => 11,
        KeyCode::Right => 12,
        KeyCode::Function(n) => 12 + n.min(12) as u32,
        KeyCode::Tab => 25,
        _ => 31,
    };
    1u32 << code.min(31)
}

fn pointer_bit(button: PointerButton) -> u32 {
    match button {
        PointerButton::Left => 1 << 2,
        PointerButton::Right => 1 << 3,
        PointerButton::Middle => 1 << 4,
        PointerButton::Other(n) => 1 << (5 + (n as u32 % 24)),
    }
}

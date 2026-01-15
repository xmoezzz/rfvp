// AUTO-GENERATED from syscalls_extracted.csv; do not edit by hand.
// Provides syscall specs and a default registration for unknown/unimplemented syscalls.

use std::collections::HashMap;

use super::Syscaller;
use crate::script::Variant;
use crate::subsystem::world::GameData;

#[derive(Debug, Clone, Copy)]
pub struct SyscallSpec {
    pub name: &'static str,
    pub group: &'static str,
    pub handler: &'static str,
    pub argc: i16,
    pub comment: &'static str,
}

pub const SYSCALL_SPECS: &[SyscallSpec] = &[
    SyscallSpec { name: "AudioLoad", group: "Audio", handler: "AudioLoad", argc: 2, comment: "" },
    SyscallSpec { name: "AudioPlay", group: "Audio", handler: "AudioPlay", argc: 2, comment: "" },
    SyscallSpec { name: "AudioSilentOn", group: "Audio", handler: "AudioSilentOn", argc: 1, comment: "" },
    SyscallSpec { name: "AudioState", group: "Audio", handler: "AudioState", argc: 1, comment: "" },
    SyscallSpec { name: "AudioStop", group: "Audio", handler: "AudioStop", argc: 2, comment: "" },
    SyscallSpec { name: "AudioType", group: "Audio", handler: "AudioType", argc: 2, comment: "" },
    SyscallSpec { name: "AudioVol", group: "Audio", handler: "AudioVol", argc: 3, comment: "" },
    SyscallSpec { name: "BREAKPOINT", group: "BREAKPOINT", handler: "nullsub_2", argc: 0, comment: "" },
    SyscallSpec { name: "ColorSet", group: "Color", handler: "ColorSet", argc: 5, comment: "" },
    SyscallSpec { name: "ControlMask", group: "Control", handler: "ControlMask", argc: 1, comment: "" },
    SyscallSpec { name: "ControlPulse", group: "Control", handler: "ControlPulse", argc: 0, comment: "" },
    SyscallSpec { name: "CursorChange", group: "Cursor", handler: "CursorChange", argc: 1, comment: "" },
    SyscallSpec { name: "CursorMove", group: "Cursor", handler: "CursorMove", argc: 3, comment: "" },
    SyscallSpec { name: "CursorShow", group: "Cursor", handler: "CursorShow", argc: 1, comment: "" },
    SyscallSpec { name: "Debmess", group: "Debmess", handler: "Debmess", argc: 2, comment: "" },
    SyscallSpec { name: "Dissolve", group: "Dissolve", handler: "Dissolve", argc: 7, comment: "" },
    SyscallSpec { name: "DissolveWait", group: "Dissolve", handler: "DissolveWait", argc: 1, comment: "" },
    SyscallSpec { name: "ExitDialog", group: "Exit", handler: "ExitDialog", argc: 0, comment: "" },
    SyscallSpec { name: "ExitMode", group: "Exit", handler: "ExitMode", argc: 1, comment: "" },
    SyscallSpec { name: "FlagGet", group: "Flag", handler: "FlagGet", argc: 1, comment: "" },
    SyscallSpec { name: "FlagSet", group: "Flag", handler: "FlagSet", argc: 2, comment: "" },
    SyscallSpec { name: "FloatToInt", group: "Float", handler: "FloatToInt", argc: 1, comment: "" },
    SyscallSpec { name: "GaijiLoad", group: "Gaiji", handler: "GaijiLoad", argc: 3, comment: "" },
    SyscallSpec { name: "GraphLoad", group: "Graph", handler: "GraphLoad", argc: 2, comment: "" },
    SyscallSpec { name: "GraphRGB", group: "Graph", handler: "GraphRGB", argc: 4, comment: "" },
    SyscallSpec { name: "HistoryGet", group: "History", handler: "HistoryGet", argc: 2, comment: "" },
    SyscallSpec { name: "HistorySet", group: "History", handler: "HistorySet", argc: 2, comment: "" },
    SyscallSpec { name: "InputFlash", group: "Input", handler: "InputFlash", argc: 0, comment: "" },
    SyscallSpec { name: "InputGetCursIn", group: "Input", handler: "InputGetCursIn", argc: 0, comment: "" },
    SyscallSpec { name: "InputGetCursX", group: "Input", handler: "InputGetCursX", argc: 0, comment: "" },
    SyscallSpec { name: "InputGetCursY", group: "Input", handler: "InputGetCursY", argc: 0, comment: "" },
    SyscallSpec { name: "InputGetDown", group: "Input", handler: "InputGetDown", argc: 0, comment: "" },
    SyscallSpec { name: "InputGetEvent", group: "Input", handler: "InputGetEvent", argc: 0, comment: "" },
    SyscallSpec { name: "InputGetRepeat", group: "Input", handler: "InputGetRepeat", argc: 0, comment: "" },
    SyscallSpec { name: "InputGetState", group: "Input", handler: "InputGetState", argc: 0, comment: "" },
    SyscallSpec { name: "InputGetUp", group: "Input", handler: "InputGetUp", argc: 0, comment: "" },
    SyscallSpec { name: "InputGetWheel", group: "Input", handler: "InputGetWheel", argc: 0, comment: "" },
    SyscallSpec { name: "InputSetClick", group: "Input", handler: "InputSetClick", argc: 1, comment: "" },
    SyscallSpec { name: "IntToText", group: "Int", handler: "IntToText", argc: 2, comment: "" },
    SyscallSpec { name: "LipAnim", group: "Lip", handler: "LipAnim", argc: 8, comment: "" },
    SyscallSpec { name: "LipSync", group: "Lip", handler: "LipSync", argc: 2, comment: "" },
    SyscallSpec { name: "Load", group: "Load", handler: "Load", argc: 1, comment: "" },
    SyscallSpec { name: "MenuMessSkip", group: "Menu", handler: "nullsub_2", argc: 1, comment: "" },
    SyscallSpec { name: "MotionAlpha", group: "Motion", handler: "MotionAlpha", argc: 6, comment: "" },
    SyscallSpec { name: "MotionAlphaStop", group: "Motion", handler: "MotionAlphaStop", argc: 1, comment: "" },
    SyscallSpec { name: "MotionAlphaTest", group: "Motion", handler: "MotionAlphaTest", argc: 1, comment: "" },
    SyscallSpec { name: "MotionAnim", group: "Motion", handler: "MotionAnim", argc: 4, comment: "" },
    SyscallSpec { name: "MotionAnimStop", group: "Motion", handler: "MotionAnimStop", argc: 1, comment: "" },
    SyscallSpec { name: "MotionAnimTest", group: "Motion", handler: "MotionAnimTest", argc: 1, comment: "" },
    SyscallSpec { name: "MotionMove", group: "Motion", handler: "MotionMove", argc: 8, comment: "" },
    SyscallSpec { name: "MotionMoveR", group: "Motion", handler: "MotionMoveR", argc: 6, comment: "" },
    SyscallSpec { name: "MotionMoveRStop", group: "Motion", handler: "MotionMoveRStop", argc: 1, comment: "" },
    SyscallSpec { name: "MotionMoveRTest", group: "Motion", handler: "MotionMoveRTest", argc: 1, comment: "" },
    SyscallSpec { name: "MotionMoveS2", group: "Motion", handler: "MotionMoveS2", argc: 8, comment: "" },
    SyscallSpec { name: "MotionMoveS2Stop", group: "Motion", handler: "MotionMoveS2Stop", argc: 1, comment: "" },
    SyscallSpec { name: "MotionMoveS2Test", group: "Motion", handler: "MotionMoveS2Test", argc: 1, comment: "" },
    SyscallSpec { name: "MotionMoveStop", group: "Motion", handler: "MotionMoveStop", argc: 1, comment: "" },
    SyscallSpec { name: "MotionMoveTest", group: "Motion", handler: "MotionMoveTest", argc: 1, comment: "" },
    SyscallSpec { name: "MotionMoveZ", group: "Motion", handler: "MotionMoveZ", argc: 6, comment: "" },
    SyscallSpec { name: "MotionMoveZStop", group: "Motion", handler: "MotionMoveZStop", argc: 1, comment: "" },
    SyscallSpec { name: "MotionMoveZTest", group: "Motion", handler: "MotionMoveZTest", argc: 1, comment: "" },
    SyscallSpec { name: "MotionPause", group: "Motion", handler: "MotionPause", argc: 2, comment: "" },
    SyscallSpec { name: "Movie", group: "Movie", handler: "Movie", argc: 2, comment: "" },
    SyscallSpec { name: "MovieState", group: "Movie", handler: "MovieState", argc: 1, comment: "" },
    SyscallSpec { name: "MovieStop", group: "Movie", handler: "MovieStop", argc: 0, comment: "" },
    SyscallSpec { name: "PartsAssign", group: "Parts", handler: "PartsAssign", argc: 2, comment: "" },
    SyscallSpec { name: "PartsLoad", group: "Parts", handler: "PartsLoad", argc: 2, comment: "" },
    SyscallSpec { name: "PartsMotion", group: "Parts", handler: "PartsMotion", argc: 3, comment: "" },
    SyscallSpec { name: "PartsMotionPause", group: "Parts", handler: "PartsMotionPause", argc: 2, comment: "" },
    SyscallSpec { name: "PartsMotionStop", group: "Parts", handler: "PartsMotionStop", argc: 1, comment: "" },
    SyscallSpec { name: "PartsMotionTest", group: "Parts", handler: "PartsMotionTest", argc: 1, comment: "" },
    SyscallSpec { name: "PartsRGB", group: "Parts", handler: "PartsRGB", argc: 4, comment: "" },
    SyscallSpec { name: "PartsSelect", group: "Parts", handler: "PartsSelect", argc: 2, comment: "" },
    SyscallSpec { name: "PrimExitGroup", group: "Prim", handler: "PrimExitGroup", argc: 1, comment: "" },
    SyscallSpec { name: "PrimGroupIn", group: "Prim", handler: "PrimGroupIn", argc: 2, comment: "" },
    SyscallSpec { name: "PrimGroupMove", group: "Prim", handler: "PrimGroupMove", argc: 2, comment: "" },
    SyscallSpec { name: "PrimGroupOut", group: "Prim", handler: "PrimGroupOut", argc: 1, comment: "" },
    SyscallSpec { name: "PrimHit", group: "Prim", handler: "PrimHit", argc: 2, comment: "" },
    SyscallSpec { name: "PrimSetAlpha", group: "Prim", handler: "PrimSetAlpha", argc: 2, comment: "" },
    SyscallSpec { name: "PrimSetBlend", group: "Prim", handler: "PrimSetBlend", argc: 2, comment: "" },
    SyscallSpec { name: "PrimSetDraw", group: "Prim", handler: "PrimSetDraw", argc: 2, comment: "" },
    SyscallSpec { name: "PrimSetNull", group: "Prim", handler: "PrimSetNull", argc: 1, comment: "" },
    SyscallSpec { name: "PrimSetOP", group: "Prim", handler: "PrimSetOP", argc: 3, comment: "" },
    SyscallSpec { name: "PrimSetRS", group: "Prim", handler: "PrimSetRS", argc: 3, comment: "" },
    SyscallSpec { name: "PrimSetRS2", group: "Prim", handler: "PrimSetRS2", argc: 4, comment: "" },
    SyscallSpec { name: "PrimSetSnow", group: "Prim", handler: "PrimSetSnow", argc: 4, comment: "" },
    SyscallSpec { name: "PrimSetSprt", group: "Prim", handler: "PrimSetSprt", argc: 4, comment: "" },
    SyscallSpec { name: "PrimSetText", group: "Prim", handler: "PrimSetText", argc: 4, comment: "" },
    SyscallSpec { name: "PrimSetTile", group: "Prim", handler: "PrimSetTile", argc: 6, comment: "" },
    SyscallSpec { name: "PrimSetUV", group: "Prim", handler: "PrimSetUV", argc: 3, comment: "" },
    SyscallSpec { name: "PrimSetWH", group: "Prim", handler: "PrimSetWH", argc: 3, comment: "" },
    SyscallSpec { name: "PrimSetXY", group: "Prim", handler: "PrimSetXY", argc: 3, comment: "" },
    SyscallSpec { name: "PrimSetZ", group: "Prim", handler: "PrimSetZ", argc: 2, comment: "" },
    SyscallSpec { name: "Rand", group: "Rand", handler: "Rand", argc: 0, comment: "" },
    SyscallSpec { name: "SaveCreate", group: "Save", handler: "SaveCreate", argc: 2, comment: "" },
    SyscallSpec { name: "SaveData", group: "Save", handler: "SaveData", argc: 3, comment: "" },
    SyscallSpec { name: "SaveThumbSize", group: "Save", handler: "SaveThumbSize", argc: 2, comment: "" },
    SyscallSpec { name: "SaveWrite", group: "Save", handler: "SaveWrite", argc: 1, comment: "" },
    SyscallSpec { name: "Snow", group: "Snow", handler: "Snow", argc: 18, comment: "" },
    SyscallSpec { name: "SnowStart", group: "Snow", handler: "SnowStart", argc: 1, comment: "" },
    SyscallSpec { name: "SnowStop", group: "Snow", handler: "SnowStop", argc: 1, comment: "" },
    SyscallSpec { name: "SoundLoad", group: "Sound", handler: "SoundLoad", argc: 2, comment: "" },
    SyscallSpec { name: "SoundMasterVol", group: "Sound", handler: "SoundMasterVol", argc: 1, comment: "" },
    SyscallSpec { name: "SoundPlay", group: "Sound", handler: "SoundPlay", argc: 3, comment: "" },
    SyscallSpec { name: "SoundSilentOn", group: "Sound", handler: "SoundSilentOn", argc: 1, comment: "" },
    SyscallSpec { name: "SoundStop", group: "Sound", handler: "SoundStop", argc: 2, comment: "" },
    SyscallSpec { name: "SoundType", group: "Sound", handler: "SoundType", argc: 2, comment: "" },
    SyscallSpec { name: "SoundTypeVol", group: "Sound", handler: "SoundTypeVol", argc: 2, comment: "" },
    SyscallSpec { name: "SoundVol", group: "Sound", handler: "SoundVol", argc: 3, comment: "" },
    SyscallSpec { name: "SysAtSkipName", group: "Sys", handler: "nullsub_2", argc: 2, comment: "" },
    SyscallSpec { name: "SysProjFolder", group: "Sys", handler: "SysProjFolder", argc: 1, comment: "" },
    SyscallSpec { name: "TextBuff", group: "Text", handler: "TextBuff", argc: 3, comment: "" },
    SyscallSpec { name: "TextClear", group: "Text", handler: "TextClear", argc: 1, comment: "" },
    SyscallSpec { name: "TextColor", group: "Text", handler: "TextColor", argc: 4, comment: "" },
    SyscallSpec { name: "TextFont", group: "Text", handler: "TextFont", argc: 3, comment: "" },
    SyscallSpec { name: "TextFontCount", group: "Text", handler: "TextFontCount", argc: 0, comment: "" },
    SyscallSpec { name: "TextFontGet", group: "Text", handler: "TextFontGet", argc: 0, comment: "" },
    SyscallSpec { name: "TextFontName", group: "Text", handler: "TextFontName", argc: 1, comment: "" },
    SyscallSpec { name: "TextFontSet", group: "Text", handler: "TextFontSet", argc: 1, comment: "" },
    SyscallSpec { name: "TextFormat", group: "Text", handler: "TextFormat", argc: 7, comment: "" },
    SyscallSpec { name: "TextFunction", group: "Text", handler: "TextFunction", argc: 4, comment: "" },
    SyscallSpec { name: "TextOutSize", group: "Text", handler: "TextOutSize", argc: 3, comment: "" },
    SyscallSpec { name: "TextPause", group: "Text", handler: "TextPause", argc: 2, comment: "" },
    SyscallSpec { name: "TextPos", group: "Text", handler: "TextPos", argc: 3, comment: "" },
    SyscallSpec { name: "TextPrint", group: "Text", handler: "TextPrint", argc: 2, comment: "" },
    SyscallSpec { name: "TextRepaint", group: "Text", handler: "TextRepaint", argc: 0, comment: "" },
    SyscallSpec { name: "TextShadowDist", group: "Text", handler: "TextShadowDist", argc: 2, comment: "" },
    SyscallSpec { name: "TextSize", group: "Text", handler: "TextSize", argc: 3, comment: "" },
    SyscallSpec { name: "TextSkip", group: "Text", handler: "TextSkip", argc: 2, comment: "" },
    SyscallSpec { name: "TextSpace", group: "Text", handler: "TextSpace", argc: 3, comment: "" },
    SyscallSpec { name: "TextSpeed", group: "Text", handler: "TextSpeed", argc: 2, comment: "" },
    SyscallSpec { name: "TextSuspendChr", group: "Text", handler: "TextSuspendChr", argc: 2, comment: "" },
    SyscallSpec { name: "TextTest", group: "Text", handler: "TextTest", argc: 1, comment: "" },
    SyscallSpec { name: "ThreadExit", group: "Thread", handler: "ThreadExit", argc: 1, comment: "" },
    SyscallSpec { name: "ThreadNext", group: "Thread", handler: "ThreadNext", argc: 0, comment: "" },
    SyscallSpec { name: "ThreadRaise", group: "Thread", handler: "ThreadRaise", argc: 1, comment: "" },
    SyscallSpec { name: "ThreadSleep", group: "Thread", handler: "ThreadSleep", argc: 1, comment: "" },
    SyscallSpec { name: "ThreadStart", group: "Thread", handler: "ThreadStart", argc: 2, comment: "" },
    SyscallSpec { name: "ThreadWait", group: "Thread", handler: "ThreadWait", argc: 1, comment: "" },
    SyscallSpec { name: "TimerGet", group: "Timer", handler: "TimerGet", argc: 2, comment: "" },
    SyscallSpec { name: "TimerSet", group: "Timer", handler: "TimerSet", argc: 2, comment: "" },
    SyscallSpec { name: "TimerSuspend", group: "Timer", handler: "TimerSuspend", argc: 1, comment: "" },
    SyscallSpec { name: "TitleMenu", group: "Title", handler: "nullsub_2", argc: 1, comment: "" },
    SyscallSpec { name: "V3DMotion", group: "V3", handler: "V3DMotion", argc: 6, comment: "" },
    SyscallSpec { name: "V3DMotionPause", group: "V3", handler: "V3DMotionPause", argc: 1, comment: "" },
    SyscallSpec { name: "V3DMotionStop", group: "V3", handler: "V3DMotionStop", argc: 0, comment: "" },
    SyscallSpec { name: "V3DMotionTest", group: "V3", handler: "V3DMotionTest", argc: 0, comment: "" },
    SyscallSpec { name: "V3DSet", group: "V3", handler: "V3DSet", argc: 3, comment: "" },
    SyscallSpec { name: "WindowMode", group: "Window", handler: "WindowMode", argc: 1, comment: "" },
];

#[derive(Debug)]
pub struct UnimplementedSyscall {
    pub name: &'static str,
    pub argc: i16,
}

impl UnimplementedSyscall {
    pub const fn new(name: &'static str, argc: i16) -> Self {
        Self { name, argc }
    }
}

impl Syscaller for UnimplementedSyscall {
    fn call(&self, _gd: &mut GameData, args: Vec<Variant>) -> anyhow::Result<Variant> {
        // Keep behavior lenient for now; correctness will be validated later against the real engine.
        if self.argc >= 0 && args.len() != self.argc as usize {
            log::warn!("syscall {} expected {} args, got {}", self.name, self.argc, args.len());
        } else {
            log::warn!("syscall {} is unimplemented (args_len={})", self.name, args.len());
        }
        Ok(Variant::Nil)
    }
}

/// Insert stub syscalls for every extracted symbol that is not already implemented.
pub fn register_unimplemented_syscalls(m: &mut HashMap<String, Box<dyn Syscaller + Send + Sync>>) {
    for s in SYSCALL_SPECS {
        m.entry(s.name.to_string()).or_insert_with(|| Box::new(UnimplementedSyscall::new(s.name, s.argc)));
    }
}
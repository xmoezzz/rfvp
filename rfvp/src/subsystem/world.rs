use std::collections::{HashMap, HashSet};
use std::sync::Arc;

use crate::audio_player::{BgmPlayer, SePlayer};
use crate::script::parser::Nls;
use crate::script::{Variant, VmSyscall};
use crate::subsystem::components::syscalls::graph::{
    PrimExitGroup, PrimGroupIn, PrimGroupMove, PrimGroupOut, PrimSetAlpha, PrimSetBlend,
    PrimSetDraw, PrimSetNull, PrimSetOP, PrimSetRS, PrimSetRS2, PrimSetSnow, PrimSetSprt,
    PrimSetText, PrimSetTile, PrimSetUV, PrimSetWH, PrimSetXY, PrimSetZ, PrimHit,
    GraphLoad, GraphRGB, GaijiLoad,
};
use crate::subsystem::components::syscalls::history::{
    HistoryGet, HistorySet
};
use crate::subsystem::components::syscalls::flag::{
    FlagSet, FlagGet
};
use crate::subsystem::components::syscalls::utils::{
    IntToText, Rand, SysProjFolder, SysAtSkipName, DebugMessage,
    BreakPoint, FloatToInt
};
use crate::subsystem::components::syscalls::thread::{
    ThreadExit, ThreadNext, ThreadRaise, ThreadSleep,
    ThreadStart, ThreadWait
};
use crate::subsystem::components::syscalls::sound::{
    SoundPlay, SoundStop, SoundLoad, SoundMasterVol, SoundSlientOn, SoundType,
    SoundTypeVol, SoundVolume, AudioLoad, AudioPlay, AudioSlientOn, AudioStop,
    AudioState, AudioType, AudioVol
};
use crate::subsystem::components::syscalls::motion::{
    MotionAlpha, MotionAlphaStop, MotionAlphaTest,
    MotionMove, MotionMoveStop, MotionMoveTest,
    MotionMoveR, MotionMoveRStop, MotionMoveRTest,
    MotionMoveS2, MotionMoveS2Stop, MotionMoveS2Test,
    MotionMoveZ, MotionMoveZStop, MotionMoveZTest, MotionPause,
    V3DMotion, V3DMotionStop, V3DMotionTest, V3DMotionPause, V3DSet
};
use crate::subsystem::components::syscalls::color::ColorSet;
use crate::subsystem::components::syscalls::input::{
    InputFlash, InputGetCursIn, InputGetCursX, InputGetCursY,
    InputGetDown, InputGetEvent, InputGetRepeat, InputGetState,
    InputGetUp, InputGetWheel, InputSetClick,
    ControlMask, ControlPulse
};
use crate::subsystem::components::syscalls::timer::{
    TimerSet, TimerGet, TimerSuspend
};
use crate::subsystem::components::syscalls::movie::{
    MoviePlay, MovieState, MovieStop
};
use crate::subsystem::components::syscalls::parts::{
    PartsLoad, PartsRGB, PartsMotion, PartsMotionTest, 
    PartsMotionStop, PartsMotionPause, PartsAssign, PartsSelect
};
use crate::subsystem::components::syscalls::text::{
    TextBuff, TextClear, TextColor, TextFont, TextFontCount,
    TextFontGet, TextFontName, TextFontSet, TextFormat,
    TextFunction, TextOutSize, TextPause, TextPos, TextPrint,
    TextReprint, TextShadowDist, TextSize, TextSkip, TextSpace,
    TextSpeed, TextSuspendChr, TextTest
};
use crate::subsystem::components::syscalls::saveload::{
    SaveCreate, SaveThumbSize, SaveWrite, SaveData, Load,
};
use crate::subsystem::components::syscalls::other_anm::{
    LipAnim, LipSync, Dissolve, Snow, SnowStart, SnowStop,
};
use crate::subsystem::components::syscalls::utils::{
    WindowMode, ExitMode
};

use crate::subsystem::resources::motion_manager::MotionManager;
use crate::subsystem::resources::time::Time;
use crate::subsystem::resources::window::Window;
use crate::subsystem::scene::SceneController;
use atomic_refcell::{AtomicRefCell, AtomicRefMut};
use hecs::{
    Component, ComponentError, DynamicBundle, Entity, NoSuchEntity, Query, QueryBorrow, QueryMut,
    QueryOne, QueryOneError,
};
use rfvp_audio::AudioManager;


use super::resources::flag_manager::FlagManager;
use super::resources::history_manager::HistoryManager;
use super::resources::input_manager::InputManager;
use super::resources::save_manager::SaveManager;
use super::resources::text_manager::FontEnumerator;

use super::resources::thread_wrapper::ThreadWrapper;
use super::resources::timer_manager::TimerManager;
use super::resources::vfs::Vfs;
use super::resources::color_manager::ColorManager;
use super::resources::videoplayer::VideoPlayerManager;

use crate::subsystem::components::syscalls::Syscaller;

pub trait World {
    fn entities(&self) -> HashSet<Entity>;
    fn clear(&mut self);
    fn push(&mut self, components: impl DynamicBundle) -> Entity;
    fn remove(&mut self, entity: Entity) -> Result<(), NoSuchEntity>;
    fn add_components(
        &mut self,
        entity: Entity,
        components: impl DynamicBundle,
    ) -> Result<(), NoSuchEntity>;
    fn remove_component<T: Component>(&mut self, entity: Entity) -> Result<T, ComponentError>;
    fn query<Q: Query>(&self) -> QueryBorrow<'_, Q>;
    fn query_mut<Q: Query>(&mut self) -> QueryMut<'_, Q>;
    fn entry<Q: Query>(&self, entity: Entity) -> Result<QueryOne<'_, Q>, NoSuchEntity>;
    fn entry_mut<Q: Query>(&mut self, entity: Entity) -> Result<Q::Item<'_>, QueryOneError>;
    fn contains(&self, entity: Entity) -> bool;
}


pub struct GameData {
    pub(crate) vfs: Vfs,
    pub(crate) thread_wrapper: ThreadWrapper,
    pub(crate) history_manager: HistoryManager,
    pub(crate) flag_manager: FlagManager,
    pub(crate) motion_manager: MotionManager,
    pub(crate) color_manager: ColorManager,
    pub(crate) fontface_manager: FontEnumerator,
    pub(crate) inputs_manager: InputManager,
    pub(crate) timer_manager: TimerManager,
    pub(crate) video_manager: VideoPlayerManager,
    pub(crate) save_manager: SaveManager,
    pub(crate) nls: Nls,
    pub(crate) memory_cache: Vec<u8>,
    time: AtomicRefCell<Time>,
    scene_controller: AtomicRefCell<SceneController>,
    window: AtomicRefCell<Window>,
    audio_manager: Arc<AudioManager>,
    se_player: AtomicRefCell<SePlayer>,
    bgm_player: AtomicRefCell<BgmPlayer>,
}

impl Default for GameData {
    fn default() -> Self {
        let audio_manager = Arc::new(AudioManager::new());

        Self {
            vfs: Vfs::default(),
            thread_wrapper: ThreadWrapper::default(),
            history_manager: HistoryManager::default(),
            flag_manager: FlagManager::default(),
            motion_manager: MotionManager::default(),
            color_manager: ColorManager::default(),
            fontface_manager: FontEnumerator::default(),
            inputs_manager: InputManager::default(),
            timer_manager: TimerManager::default(),
            video_manager: VideoPlayerManager::default(),
            save_manager: SaveManager::default(),
            nls: Nls::default(),
            memory_cache: Vec::new(),
            time: AtomicRefCell::new(Time::default()),
            scene_controller: AtomicRefCell::new(SceneController::default()),
            window: AtomicRefCell::new(Window::default()),
            se_player: AtomicRefCell::new(SePlayer::new(audio_manager.clone())),
            bgm_player: AtomicRefCell::new(BgmPlayer::new(audio_manager.clone())),
            audio_manager,
        }
    }
}

impl GameData {
    /// retrieves the timers resource from the resources.
    pub fn time(&self) -> AtomicRefMut<Time> {
        self.time.borrow_mut()
    }

    /// retrieves the window from the resources
    pub fn window(&self) -> AtomicRefMut<Window> {
        self.window.borrow_mut()
    }

    pub fn set_window(&mut self, window: Window) {
        self.window = AtomicRefCell::new(window);
    }

    /// retrieves the window from the resources
    pub fn scene_controller(&self) -> AtomicRefMut<SceneController> {
        self.scene_controller.borrow_mut()
    }

    pub fn vfs_load_file(&self, path: &str) -> anyhow::Result<Vec<u8>> {
        self.vfs.read_file(path)
    }

    pub fn get_width(&self) -> u32 {
        self.window().width()
    }

    pub fn get_height(&self) -> u32 {
        self.window().height()
    }

    pub fn get_nls(&self) -> Nls {
        self.nls.clone()
    }

    pub fn get_cache(&self) -> Vec<u8> {
        self.memory_cache.clone()
    }

    pub fn se_player(&self) -> AtomicRefMut<SePlayer> {
        self.se_player.borrow_mut()
    }

    pub fn bgm_player(&self) -> AtomicRefMut<BgmPlayer> {
        self.bgm_player.borrow_mut()
    }

    pub fn audio_manager(&self) -> Arc<AudioManager> {
        self.audio_manager.clone()
    }
}

impl VmSyscall for GameData {
    fn do_syscall(&mut self, name: &str, args: Vec<Variant>) -> anyhow::Result<Variant> {
        let result = SYSCALL_TBL.borrow().get(name).map_or_else(
            || {
                log::error!("Syscall {} not found", name);
                Ok(Variant::Nil)
            },
            |syscall| syscall.call(self, args),
        );
        result
    }
}

// no one wants to own this:(
lazy_static::lazy_static! {
    static ref SYSCALL_TBL : AtomicRefCell<HashMap<String, Box<dyn Syscaller + 'static + Send + Sync>>> = {
        let mut m: HashMap<String, Box<dyn Syscaller + 'static + Send + Sync>> = HashMap::new();

        // audio apis
        m.insert("SoundPlay".into(), Box::new(SoundPlay));
        m.insert("SoundStop".into(), Box::new(SoundStop));
        m.insert("SoundLoad".into(), Box::new(SoundLoad));
        m.insert("SoundMasterVol".into(), Box::new(SoundMasterVol));
        m.insert("SoundSlientOn".into(), Box::new(SoundSlientOn));
        m.insert("SoundType".into(), Box::new(SoundType));
        m.insert("SoundTypeVol".into(), Box::new(SoundTypeVol));
        m.insert("SoundVolume".into(), Box::new(SoundVolume));
        m.insert("AudioLoad".into(), Box::new(AudioLoad));
        m.insert("AudioPlay".into(), Box::new(AudioPlay));
        m.insert("AudioSlientOn".into(), Box::new(AudioSlientOn));
        m.insert("AudioStop".into(), Box::new(AudioStop));
        m.insert("AudioState".into(), Box::new(AudioState));
        m.insert("AudioType".into(), Box::new(AudioType));
        m.insert("AudioVol".into(), Box::new(AudioVol));

        // utils apis
        m.insert("IntToText".into(), Box::new(IntToText));
        m.insert("Rand".into(), Box::new(Rand));
        m.insert("SysProjFolder".into(), Box::new(SysProjFolder));
        m.insert("SysAtSkipName".into(), Box::new(SysAtSkipName));
        m.insert("DebugMessage".into(), Box::new(DebugMessage));
        m.insert("BreakPoint".into(), Box::new(BreakPoint));
        m.insert("FloatToInt".into(), Box::new(FloatToInt));

        m.insert("WindowMode".into(), Box::new(WindowMode));
        m.insert("ExitMode".into(), Box::new(ExitMode));

        // thread apis
        m.insert("ThreadExit".into(), Box::new(ThreadExit));
        m.insert("ThreadNext".into(), Box::new(ThreadNext));
        m.insert("ThreadRaise".into(), Box::new(ThreadRaise));
        m.insert("ThreadSleep".into(), Box::new(ThreadSleep));
        m.insert("ThreadStart".into(), Box::new(ThreadStart));
        m.insert("ThreadWait".into(), Box::new(ThreadWait));

        // flag apis
        m.insert("FlagSet".into(), Box::new(FlagSet));
        m.insert("FlagGet".into(), Box::new(FlagGet));

        // history apis
        m.insert("HistoryGet".into(), Box::new(HistoryGet));
        m.insert("HistorySet".into(), Box::new(HistorySet));

        // prim apis
        m.insert("PrimExitGroup".into(), Box::new(PrimExitGroup));
        m.insert("PrimGroupIn".into(), Box::new(PrimGroupIn));
        m.insert("PrimGroupOut".into(), Box::new(PrimGroupOut));
        m.insert("PrimGroupMove".into(), Box::new(PrimGroupMove));
        m.insert("PrimSetNull".into(), Box::new(PrimSetNull));
        m.insert("PrimSetAlpha".into(), Box::new(PrimSetAlpha));
        m.insert("PrimSetBlend".into(), Box::new(PrimSetBlend));
        m.insert("PrimSetDraw".into(), Box::new(PrimSetDraw));
        m.insert("PrimSetOP".into(), Box::new(PrimSetOP));
        m.insert("PrimSetRS".into(), Box::new(PrimSetRS));
        m.insert("PrimSetRS2".into(), Box::new(PrimSetRS2));
        m.insert("PrimSetSnow".into(), Box::new(PrimSetSnow));
        m.insert("PrimSetSprt".into(), Box::new(PrimSetSprt));
        m.insert("PrimSetText".into(), Box::new(PrimSetText));
        m.insert("PrimSetTile".into(), Box::new(PrimSetTile));
        m.insert("PrimSetUV".into(), Box::new(PrimSetUV));
        m.insert("PrimSetXY".into(), Box::new(PrimSetXY));
        m.insert("PrimSetWH".into(), Box::new(PrimSetWH));
        m.insert("PrimSetZ".into(), Box::new(PrimSetZ));
        m.insert("PrimHit".into(), Box::new(PrimHit));

        // graph apis
        m.insert("GraphLoad".into(), Box::new(GraphLoad));
        m.insert("GraphRGB".into(), Box::new(GraphRGB));

        // gaiji apis
        m.insert("GaijiLoad".into(), Box::new(GaijiLoad));

        // motion apis
        m.insert("MotionAlpha".into(), Box::new(MotionAlpha));
        m.insert("MotionAlphaStop".into(), Box::new(MotionAlphaStop));
        m.insert("MotionAlphaTest".into(), Box::new(MotionAlphaTest));
        m.insert("MotionMove".into(), Box::new(MotionMove));
        m.insert("MotionMoveStop".into(), Box::new(MotionMoveStop));
        m.insert("MotionMoveTest".into(), Box::new(MotionMoveTest));
        m.insert("MotionMoveR".into(), Box::new(MotionMoveR));
        m.insert("MotionMoveRStop".into(), Box::new(MotionMoveRStop));
        m.insert("MotionMoveRTest".into(), Box::new(MotionMoveRTest));
        m.insert("MotionMoveS2".into(), Box::new(MotionMoveS2));
        m.insert("MotionMoveS2Stop".into(), Box::new(MotionMoveS2Stop));
        m.insert("MotionMoveS2Test".into(), Box::new(MotionMoveS2Test));
        m.insert("MotionMoveZ".into(), Box::new(MotionMoveZ));
        m.insert("MotionMoveZStop".into(), Box::new(MotionMoveZStop));
        m.insert("MotionMoveZTest".into(), Box::new(MotionMoveZTest));
        m.insert("MotionPause".into(), Box::new(MotionPause));
        m.insert("V3DMotion".into(), Box::new(V3DMotion));
        m.insert("V3DMotionStop".into(), Box::new(V3DMotionStop));
        m.insert("V3DMotionTest".into(), Box::new(V3DMotionTest));
        m.insert("V3DMotionPause".into(), Box::new(V3DMotionPause));
        m.insert("V3DSet".into(), Box::new(V3DSet));

        // color apis
        m.insert("ColorSet".into(), Box::new(ColorSet));

        // text api
        m.insert("TextBuff".into(), Box::new(TextBuff));
        m.insert("TextClear".into(), Box::new(TextClear));
        m.insert("TextColor".into(), Box::new(TextColor));
        m.insert("TextFont".into(), Box::new(TextFont));
        m.insert("TextFontCount".into(), Box::new(TextFontCount));
        m.insert("TextFontGet".into(), Box::new(TextFontGet));
        m.insert("TextFontName".into(), Box::new(TextFontName));
        m.insert("TextFontSet".into(), Box::new(TextFontSet));
        m.insert("TextFormat".into(), Box::new(TextFormat));
        m.insert("TextFunction".into(), Box::new(TextFunction));
        m.insert("TextOutSize".into(), Box::new(TextOutSize));
        m.insert("TextPause".into(), Box::new(TextPause));
        m.insert("TextPos".into(), Box::new(TextPos));
        m.insert("TextPrint".into(), Box::new(TextPrint));
        m.insert("TextReprint".into(), Box::new(TextReprint));
        m.insert("TextShadowDist".into(), Box::new(TextShadowDist));
        m.insert("TextSize".into(), Box::new(TextSize));
        m.insert("TextSkip".into(), Box::new(TextSkip));
        m.insert("TextSpace".into(), Box::new(TextSpace));
        m.insert("TextSpeed".into(), Box::new(TextSpeed));
        m.insert("TextSuspendChr".into(), Box::new(TextSuspendChr));
        m.insert("TextTest".into(), Box::new(TextTest));

        // input apis
        m.insert("InputFlash".into(), Box::new(InputFlash));
        m.insert("InputGetCursIn".into(), Box::new(InputGetCursIn));
        m.insert("InputGetCursX".into(), Box::new(InputGetCursX));
        m.insert("InputGetCursY".into(), Box::new(InputGetCursY));
        m.insert("InputGetDown".into(), Box::new(InputGetDown));
        m.insert("InputGetEvent".into(), Box::new(InputGetEvent));
        m.insert("InputGetRepeat".into(), Box::new(InputGetRepeat));
        m.insert("InputGetState".into(), Box::new(InputGetState));
        m.insert("InputGetUp".into(), Box::new(InputGetUp));
        m.insert("InputGetWheel".into(), Box::new(InputGetWheel));
        m.insert("InputSetClick".into(), Box::new(InputSetClick));
        m.insert("ControlPulse".into(), Box::new(ControlPulse));
        m.insert("ControlMask".into(), Box::new(ControlMask));

        // timer apis
        m.insert("TimerSet".into(), Box::new(TimerSet));
        m.insert("TimerGet".into(), Box::new(TimerGet));
        m.insert("TimerSuspend".into(), Box::new(TimerSuspend));

        // movie apis
        m.insert("MoviePlay".into(), Box::new(MoviePlay));
        m.insert("MovieState".into(), Box::new(MovieState));
        m.insert("MovieStop".into(), Box::new(MovieStop));

        // parts apis
        m.insert("PartsLoad".into(), Box::new(PartsLoad));
        m.insert("PartsRGB".into(), Box::new(PartsRGB));
        m.insert("PartsMotion".into(), Box::new(PartsMotion));
        m.insert("PartsMotionTest".into(), Box::new(PartsMotionTest));
        m.insert("PartsMotionStop".into(), Box::new(PartsMotionStop));
        m.insert("PartsMotionPause".into(), Box::new(PartsMotionPause));
        m.insert("PartsAssign".into(), Box::new(PartsAssign));
        m.insert("PartsSelect".into(), Box::new(PartsSelect));

        // save apis
        m.insert("SaveCreate".into(), Box::new(SaveCreate));
        m.insert("SaveThumbSize".into(), Box::new(SaveThumbSize));
        m.insert("SaveWrite".into(), Box::new(SaveWrite));
        m.insert("SaveData".into(), Box::new(SaveData));

        // load api
        m.insert("Load".into(), Box::new(Load));

        // other anm apis
        m.insert("LipAnim".into(), Box::new(LipAnim));
        m.insert("LipSync".into(), Box::new(LipSync));
        m.insert("Dissolve".into(), Box::new(Dissolve));
        m.insert("Snow".into(), Box::new(Snow));
        m.insert("SnowStart".into(), Box::new(SnowStart));
        m.insert("SnowStop".into(), Box::new(SnowStop));

        AtomicRefCell::new(m)
    };
}

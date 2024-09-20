//! Defines the commands that can be produced by the VM and executed by the engine.
use crate::format::scenario::variant::Variant;

pub mod types;

#[allow(non_camel_case_types, clippy::upper_case_acronyms)]
#[derive(Debug)]
pub enum Command {
    AudioLoad {args: Vec<Variant>},
    AudioPlay {args: Vec<Variant>},
    AudioSilentOn {args: Vec<Variant>},
    AudioState {args: Vec<Variant>},
    AudioStop {args: Vec<Variant>},
    AudioType {args: Vec<Variant>},
    AudioVol {args: Vec<Variant>},
    ColorSet {args: Vec<Variant>},
    ControlMask {args: Vec<Variant>},
    ControlPulse {args: Vec<Variant>},
    CursorChange {args: Vec<Variant>},
    CursorMove {args: Vec<Variant>},
    CursorShow {args: Vec<Variant>},
    Debmess {args: Vec<Variant>},
    Dissolve {args: Vec<Variant>},
    DissolveWait {args: Vec<Variant>},
    ExitDialog {args: Vec<Variant>},
    ExitMode {args: Vec<Variant>},
    FlagGet {args: Vec<Variant>},
    FlagSet {args: Vec<Variant>},
    FloatToInt {args: Vec<Variant>},
    GaijiLoad {args: Vec<Variant>},
    GraphLoad {args: Vec<Variant>},
    GraphRGB {args: Vec<Variant>},
    IntToText {args: Vec<Variant>},
    HistoryGet {args: Vec<Variant>},
    HistorySet {args: Vec<Variant>},
    InputFlash {args: Vec<Variant>},
    InputGetCursIn {args: Vec<Variant>},
    InputGetCursX {args: Vec<Variant>},
    InputGetCursY {args: Vec<Variant>},
    InputGetDown {args: Vec<Variant>},
    InputGetEvent {args: Vec<Variant>},
    InputGetRepeat {args: Vec<Variant>},
    InputGetState {args: Vec<Variant>},
    InputGetUp {args: Vec<Variant>},
    InputGetWheel {args: Vec<Variant>},
    InputSetClick {args: Vec<Variant>},
    LipAnim {args: Vec<Variant>},
    LipSync {args: Vec<Variant>},
    Load {args: Vec<Variant>},
    MenuMessSkip {args: Vec<Variant>},
    MotionAlpha {args: Vec<Variant>},
    MotionAlphaStop {args: Vec<Variant>},
    MotionAlphaTest {args: Vec<Variant>},
    MotionAnim {args: Vec<Variant>},
    MotionAnimStop {args: Vec<Variant>},
    MotionAnimTest {args: Vec<Variant>},
    MotionMove {args: Vec<Variant>},
    MotionMoveStop {args: Vec<Variant>},
    MotionMoveTest {args: Vec<Variant>},
    MotionMoveR {args: Vec<Variant>},
    MotionMoveRStop {args: Vec<Variant>},
    MotionMoveRTest {args: Vec<Variant>},
    MotionMoveS2 {args: Vec<Variant>},
    MotionMoveS2Stop {args: Vec<Variant>},
    MotionMoveS2Test {args: Vec<Variant>},
    MotionMoveZ {args: Vec<Variant>},
    MotionMoveZStop {args: Vec<Variant>},
    MotionMoveZTest {args: Vec<Variant>},
    MotionPause {args: Vec<Variant>},
    Movie {args: Vec<Variant>},
    MovieState {args: Vec<Variant>},
    MovieStop {args: Vec<Variant>},
    PartsAssign {args: Vec<Variant>},
    PartsLoad {args: Vec<Variant>},
    PartsMotion {args: Vec<Variant>},
    PartsMotionPause {args: Vec<Variant>},
    PartsMotionStop {args: Vec<Variant>},
    PartsMotionTest {args: Vec<Variant>},
    PartsRGB {args: Vec<Variant>},
    PartsSelect {args: Vec<Variant>},
    PrimExitGroup {args: Vec<Variant>},
    PrimGroupIn {args: Vec<Variant>},
    PrimGroupMove {args: Vec<Variant>},
    PrimGroupOut {args: Vec<Variant>},
    PrimHit {args: Vec<Variant>},
    PrimSetAlpha {args: Vec<Variant>},
    PrimSetBlend {args: Vec<Variant>},
    PrimSetDraw {args: Vec<Variant>},
    PrimSetNull {args: Vec<Variant>},
    PrimSetOP {args: Vec<Variant>},
    PrimSetRS {args: Vec<Variant>},
    PrimSetRS2 {args: Vec<Variant>},
    PrimSetSnow {args: Vec<Variant>},
    PrimSetSprt {args: Vec<Variant>},
    PrimSetText {args: Vec<Variant>},
    PrimSetTile {args: Vec<Variant>},
    PrimSetUV {args: Vec<Variant>},
    PrimSetWH {args: Vec<Variant>},
    PrimSetXY {args: Vec<Variant>},
    PrimSetZ {args: Vec<Variant>},
    Rand {args: Vec<Variant>},
    SaveCreate {args: Vec<Variant>},
    SaveThumbSize {args: Vec<Variant>},
    SaveData {args: Vec<Variant>},
    SaveWrite {args: Vec<Variant>},
    Snow {args: Vec<Variant>},
    SnowStart {args: Vec<Variant>},
    SnowStop {args: Vec<Variant>},
    SoundLoad {args: Vec<Variant>},
    SoundMasterVol {args: Vec<Variant>},
    SoundPlay {args: Vec<Variant>},
    SoundSilentOn {args: Vec<Variant>},
    SoundStop {args: Vec<Variant>},
    SoundType {args: Vec<Variant>},
    SoundTypeVol {args: Vec<Variant>},
    SoundVol {args: Vec<Variant>},
    SysAtSkipName {args: Vec<Variant>},
    SysProjFolder {args: Vec<Variant>},
    TextBuff {args: Vec<Variant>},
    TextClear {args: Vec<Variant>},
    TextColor {args: Vec<Variant>},
    TextFont {args: Vec<Variant>},
    TextFontCount {args: Vec<Variant>},
    TextFontGet {args: Vec<Variant>},
    TextFontName {args: Vec<Variant>},
    TextFontSet {args: Vec<Variant>},
    TextFormat {args: Vec<Variant>},
    TextFunction {args: Vec<Variant>},
    TextOutSize {args: Vec<Variant>},
    TextPause {args: Vec<Variant>},
    TextPos {args: Vec<Variant>},
    TextPrint {args: Vec<Variant>},
    TextRepaint {args: Vec<Variant>},
    TextShadowDist {args: Vec<Variant>},
    TextSize {args: Vec<Variant>},
    TextSkip {args: Vec<Variant>},
    TextSpace {args: Vec<Variant>},
    TextSpeed {args: Vec<Variant>},
    TextSuspendChr {args: Vec<Variant>},
    TextTest {args: Vec<Variant>},
    ThreadExit {args: Vec<Variant>},
    ThreadNext {args: Vec<Variant>},
    ThreadRaise {args: Vec<Variant>},
    ThreadSleep {args: Vec<Variant>},
    ThreadStart {args: Vec<Variant>},
    ThreadWait {args: Vec<Variant>},
    TimerGet {args: Vec<Variant>},
    TimerSet {args: Vec<Variant>},
    TimerSuspend {args: Vec<Variant>},
    TitleMenu {args: Vec<Variant>},
    V3DMotion {args: Vec<Variant>},
    V3DMotionPause {args: Vec<Variant>},
    V3DMotionStop {args: Vec<Variant>},
    V3DMotionTest {args: Vec<Variant>},
    V3DSet {args: Vec<Variant>},
    WindowMode {args: Vec<Variant>},
}

#[derive(Debug)]
pub enum RuntimeCommand {
    
}


/// An untyped result of a command execution. This is usually obtained by using a command token.
#[derive(Debug, Clone)]
pub enum CommandResult {
    /// No result
    None,
    /// Write back a value to R0 (aka the return value)
    WriteR0(Variant),
}
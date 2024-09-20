#![allow(clippy::upper_case_acronyms)]

mod prelude {
    pub use std::sync::Arc;

    pub use rfvp_core::{
        format::scenario::Scenario,
        time::Ticks,
        vm::{
            command,
            command::{
                types::{VLayerId, VLayerIdRepr},
                CommandResult,
            },
        },
    };
    pub use tracing::warn;
    pub use CommandStartResult::Yield;

    pub use crate::{
        adv::{AdvState, CommandStartResult, StartableCommand, UpdatableCommand, VmState},
        layer::Layer,
        update::UpdateContext,
    };
}

mod autosave;
mod bgmplay;
mod bgmstop;
mod bgmvol;
mod chars;
mod debugout;
mod evbegin;
mod evend;
mod layerctrl;
mod layerinit;
mod layerload;
mod layerselect;
mod layerunload;
mod layerwait;
mod moviewait;
mod msgclose;
mod msginit;
mod msgset;
mod msgsignal;
mod msgwait;
mod notifyset;
mod pageback;
mod planeclear;
mod planeselect;
mod saveinfo;
mod sepan;
mod seplay;
mod sestop;
mod sestopall;
mod sevol;
mod sewait;
mod sget;
mod showchars;
mod sset;
mod tipsget;
mod trophy;
mod unlock;
mod voiceplay;
mod wait;
mod wipe;

use std::sync::Arc;

use derivative::Derivative;
use enum_dispatch::enum_dispatch;
use layerload::LAYERLOAD;
use layerwait::LAYERWAIT;
use moviewait::MOVIEWAIT;
use msgset::MSGSET;
use msgwait::MSGWAIT;
use sewait::SEWAIT;
use rfvp_core::{
    format::scenario::Scenario,
    vm::command::{CommandResult, RuntimeCommand},
};
use wait::WAIT;

use crate::{
    adv::{AdvState, VmState},
    update::UpdateContext,
};

#[enum_dispatch]
pub trait UpdatableCommand {
    // TODO: provide mutable access to Adv Scene state
    fn update(
        &mut self,
        _context: &UpdateContext,
        _scenario: &Arc<Scenario>,
        _vm_state: &VmState,
        adv_state: &mut AdvState,
        is_fast_forwarding: bool,
    ) -> Option<CommandResult>;
}

// all commands that yield to the game loop should have:
// - a type implementing UpdatableCommand
// - a enum variant for that type here
#[enum_dispatch(UpdatableCommand)]
#[derive(Derivative)]
#[derivative(Debug)]
pub enum ExecutingCommand {
    #[derivative(Debug = "transparent")]
    WAIT,
    #[derivative(Debug = "transparent")]
    MSGSET,
    #[derivative(Debug = "transparent")]
    MSGWAIT,
    #[derivative(Debug = "transparent")]
    LAYERLOAD,
    #[derivative(Debug = "transparent")]
    LAYERWAIT,
    #[derivative(Debug = "transparent")]
    SEWAIT,
    #[derivative(Debug = "transparent")]
    MOVIEWAIT,
}

impl StartableCommand for RuntimeCommand {
    fn apply_state(&self, state: &mut VmState) {
        match self {
            // RuntimeCommand::EXIT(v) => v.apply_state(state),
            RuntimeCommand::SGET(v) => v.apply_state(state),
            RuntimeCommand::SSET(v) => v.apply_state(state),
            RuntimeCommand::WAIT(v) => v.apply_state(state),
            RuntimeCommand::MSGINIT(v) => v.apply_state(state),
            RuntimeCommand::MSGSET(v) => v.apply_state(state),
            RuntimeCommand::MSGWAIT(v) => v.apply_state(state),
            RuntimeCommand::MSGSIGNAL(v) => v.apply_state(state),
            // RuntimeCommand::MSGSYNC(v) => v.apply_state(state),
            RuntimeCommand::MSGCLOSE(v) => v.apply_state(state),
            // RuntimeCommand::SELECT(v) => v.apply_state(state),
            RuntimeCommand::WIPE(v) => v.apply_state(state),
            // RuntimeCommand::WIPEWAIT(v) => v.apply_state(state),
            RuntimeCommand::BGMPLAY(v) => v.apply_state(state),
            RuntimeCommand::BGMSTOP(v) => v.apply_state(state),
            RuntimeCommand::BGMVOL(v) => v.apply_state(state),
            // RuntimeCommand::BGMWAIT(v) => v.apply_state(state),
            // RuntimeCommand::BGMSYNC(v) => v.apply_state(state),
            RuntimeCommand::SEPLAY(v) => v.apply_state(state),
            RuntimeCommand::SESTOP(v) => v.apply_state(state),
            RuntimeCommand::SESTOPALL(v) => v.apply_state(state),
            RuntimeCommand::SEVOL(v) => v.apply_state(state),
            RuntimeCommand::SEPAN(v) => v.apply_state(state),
            RuntimeCommand::SEWAIT(v) => v.apply_state(state),
            // RuntimeCommand::SEONCE(v) => v.apply_state(state),
            RuntimeCommand::VOICEPLAY(v) => v.apply_state(state),
            // RuntimeCommand::VOICESTOP(v) => v.apply_state(state),
            // RuntimeCommand::VOICEWAIT(v) => v.apply_state(state),
            // RuntimeCommand::SYSSE(v) => v.apply_state(state),
            RuntimeCommand::SAVEINFO(v) => v.apply_state(state),
            RuntimeCommand::AUTOSAVE(v) => v.apply_state(state),
            RuntimeCommand::EVBEGIN(v) => v.apply_state(state),
            RuntimeCommand::EVEND(v) => v.apply_state(state),
            // RuntimeCommand::RESUMESET(v) => v.apply_state(state),
            // RuntimeCommand::RESUME(v) => v.apply_state(state),
            // RuntimeCommand::SYSCALL(v) => v.apply_state(state),
            RuntimeCommand::TROPHY(v) => v.apply_state(state),
            RuntimeCommand::UNLOCK(v) => v.apply_state(state),
            RuntimeCommand::LAYERINIT(v) => v.apply_state(state),
            RuntimeCommand::LAYERLOAD(v) => v.apply_state(state),
            RuntimeCommand::LAYERUNLOAD(v) => v.apply_state(state),
            RuntimeCommand::LAYERCTRL(v) => v.apply_state(state),
            RuntimeCommand::LAYERWAIT(v) => v.apply_state(state),
            // RuntimeCommand::LAYERSWAP(v) => v.apply_state(state),
            RuntimeCommand::LAYERSELECT(v) => v.apply_state(state),
            RuntimeCommand::MOVIEWAIT(v) => v.apply_state(state),
            // RuntimeCommand::TRANSSET(v) => v.apply_state(state),
            // RuntimeCommand::TRANSWAIT(v) => v.apply_state(state),
            RuntimeCommand::PAGEBACK(v) => v.apply_state(state),
            RuntimeCommand::PLANESELECT(v) => v.apply_state(state),
            RuntimeCommand::PLANECLEAR(v) => v.apply_state(state),
            // RuntimeCommand::MASKLOAD(v) => v.apply_state(state),
            // RuntimeCommand::MASKUNLOAD(v) => v.apply_state(state),
            RuntimeCommand::CHARS(v) => v.apply_state(state),
            RuntimeCommand::TIPSGET(v) => v.apply_state(state),
            // RuntimeCommand::QUIZ(v) => v.apply_state(state),
            RuntimeCommand::SHOWCHARS(v) => v.apply_state(state),
            RuntimeCommand::NOTIFYSET(v) => v.apply_state(state),
            RuntimeCommand::DEBUGOUT(v) => v.apply_state(state),
            _ => todo!(),
        }
    }

    fn start(
        self,
        context: &UpdateContext,
        scenario: &Arc<Scenario>,
        vm_state: &VmState,
        adv_state: &mut AdvState,
    ) -> CommandStartResult {
        match self {
            // RuntimeCommand::EXIT(v) => v.start(context, scenario, vm_state, adv_state),
            RuntimeCommand::SGET(v) => v.start(context, scenario, vm_state, adv_state),
            RuntimeCommand::SSET(v) => v.start(context, scenario, vm_state, adv_state),
            RuntimeCommand::WAIT(v) => v.start(context, scenario, vm_state, adv_state),
            RuntimeCommand::MSGINIT(v) => v.start(context, scenario, vm_state, adv_state),
            RuntimeCommand::MSGSET(v) => v.start(context, scenario, vm_state, adv_state),
            RuntimeCommand::MSGWAIT(v) => v.start(context, scenario, vm_state, adv_state),
            RuntimeCommand::MSGSIGNAL(v) => v.start(context, scenario, vm_state, adv_state),
            // RuntimeCommand::MSGSYNC(v) => v.start(context, scenario, vm_state, adv_state),
            RuntimeCommand::MSGCLOSE(v) => v.start(context, scenario, vm_state, adv_state),
            // RuntimeCommand::SELECT(v) => v.start(context, scenario, vm_state, adv_state),
            RuntimeCommand::WIPE(v) => v.start(context, scenario, vm_state, adv_state),
            // RuntimeCommand::WIPEWAIT(v) => v.start(context, scenario, vm_state, adv_state),
            RuntimeCommand::BGMPLAY(v) => v.start(context, scenario, vm_state, adv_state),
            RuntimeCommand::BGMSTOP(v) => v.start(context, scenario, vm_state, adv_state),
            RuntimeCommand::BGMVOL(v) => v.start(context, scenario, vm_state, adv_state),
            // RuntimeCommand::BGMWAIT(v) => v.start(context, scenario, vm_state, adv_state),
            // RuntimeCommand::BGMSYNC(v) => v.start(context, scenario, vm_state, adv_state),
            RuntimeCommand::SEPLAY(v) => v.start(context, scenario, vm_state, adv_state),
            RuntimeCommand::SESTOP(v) => v.start(context, scenario, vm_state, adv_state),
            RuntimeCommand::SESTOPALL(v) => v.start(context, scenario, vm_state, adv_state),
            RuntimeCommand::SEVOL(v) => v.start(context, scenario, vm_state, adv_state),
            RuntimeCommand::SEPAN(v) => v.start(context, scenario, vm_state, adv_state),
            RuntimeCommand::SEWAIT(v) => v.start(context, scenario, vm_state, adv_state),
            // RuntimeCommand::SEONCE(v) => v.start(context, scenario, vm_state, adv_state),
            RuntimeCommand::VOICEPLAY(v) => v.start(context, scenario, vm_state, adv_state),
            // RuntimeCommand::VOICESTOP(v) => v.start(context, scenario, vm_state, adv_state),
            // RuntimeCommand::VOICEWAIT(v) => v.start(context, scenario, vm_state, adv_state),
            // RuntimeCommand::SYSSE(v) => v.start(context, scenario, vm_state, adv_state),
            RuntimeCommand::SAVEINFO(v) => v.start(context, scenario, vm_state, adv_state),
            RuntimeCommand::AUTOSAVE(v) => v.start(context, scenario, vm_state, adv_state),
            RuntimeCommand::EVBEGIN(v) => v.start(context, scenario, vm_state, adv_state),
            RuntimeCommand::EVEND(v) => v.start(context, scenario, vm_state, adv_state),
            // RuntimeCommand::RESUMESET(v) => v.start(context, scenario, vm_state, adv_state),
            // RuntimeCommand::RESUME(v) => v.start(context, scenario, vm_state, adv_state),
            // RuntimeCommand::SYSCALL(v) => v.start(context, scenario, vm_state, adv_state),
            RuntimeCommand::TROPHY(v) => v.start(context, scenario, vm_state, adv_state),
            RuntimeCommand::UNLOCK(v) => v.start(context, scenario, vm_state, adv_state),
            RuntimeCommand::LAYERINIT(v) => v.start(context, scenario, vm_state, adv_state),
            RuntimeCommand::LAYERLOAD(v) => v.start(context, scenario, vm_state, adv_state),
            RuntimeCommand::LAYERUNLOAD(v) => v.start(context, scenario, vm_state, adv_state),
            RuntimeCommand::LAYERCTRL(v) => v.start(context, scenario, vm_state, adv_state),
            RuntimeCommand::LAYERWAIT(v) => v.start(context, scenario, vm_state, adv_state),
            // RuntimeCommand::LAYERSWAP(v) => v.start(context, scenario, vm_state, adv_state),
            RuntimeCommand::LAYERSELECT(v) => v.start(context, scenario, vm_state, adv_state),
            RuntimeCommand::MOVIEWAIT(v) => v.start(context, scenario, vm_state, adv_state),
            // RuntimeCommand::TRANSSET(v) => v.start(context, scenario, vm_state, adv_state),
            // RuntimeCommand::TRANSWAIT(v) => v.start(context, scenario, vm_state, adv_state),
            RuntimeCommand::PAGEBACK(v) => v.start(context, scenario, vm_state, adv_state),
            RuntimeCommand::PLANESELECT(v) => v.start(context, scenario, vm_state, adv_state),
            RuntimeCommand::PLANECLEAR(v) => v.start(context, scenario, vm_state, adv_state),
            // RuntimeCommand::MASKLOAD(v) => v.start(context, scenario, vm_state, adv_state),
            // RuntimeCommand::MASKUNLOAD(v) => v.start(context, scenario, vm_state, adv_state),
            RuntimeCommand::CHARS(v) => v.start(context, scenario, vm_state, adv_state),
            RuntimeCommand::TIPSGET(v) => v.start(context, scenario, vm_state, adv_state),
            // RuntimeCommand::QUIZ(v) => v.start(context, scenario, vm_state, adv_state),
            RuntimeCommand::SHOWCHARS(v) => v.start(context, scenario, vm_state, adv_state),
            RuntimeCommand::NOTIFYSET(v) => v.start(context, scenario, vm_state, adv_state),
            RuntimeCommand::DEBUGOUT(v) => v.start(context, scenario, vm_state, adv_state),
            _ => todo!(),
        }
    }
}

pub enum CommandStartResult {
    /// Continue VM execution
    Continue(CommandResult),
    /// Yield to the game loop, run the command to completion, execution continued with the result
    Yield(ExecutingCommand),
    #[allow(unused)] // TODO: it will be used for implementing the "EXIT" command
    Exit,
}

impl From<CommandResult> for CommandStartResult {
    fn from(result: CommandResult) -> Self {
        CommandStartResult::Continue(result)
    }
}

impl From<ExecutingCommand> for CommandStartResult {
    fn from(command: ExecutingCommand) -> Self {
        CommandStartResult::Yield(command)
    }
}

pub trait StartableCommand {
    fn apply_state(&self, state: &mut VmState);
    fn start(
        self,
        context: &UpdateContext,
        scenario: &Arc<Scenario>,
        vm_state: &VmState,
        adv_state: &mut AdvState,
    ) -> CommandStartResult;
}

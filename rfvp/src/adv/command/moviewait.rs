use std::fmt::Debug;

use rfvp_core::vm::command::types::LayerId;

use super::prelude::*;
use crate::layer::UserLayer;

pub struct MOVIEWAIT {
    token: Option<command::token::MOVIEWAIT>,
    layer_id: LayerId,
    // target_status: AudioWaitStatus,
}

impl StartableCommand for command::runtime::MOVIEWAIT {
    fn apply_state(&self, _state: &mut VmState) {}

    fn start(
        self,
        _context: &UpdateContext,
        _scenario: &Arc<Scenario>,
        vm_state: &VmState,
        adv_state: &mut AdvState,
    ) -> CommandStartResult {
        match adv_state.get_layer(vm_state, self.layer_id) {
            Some(UserLayer::MovieLayer(_)) => {
                assert_eq!(self.target_status, 2, "MOVIEWAIT: unknown target status");
                Yield(
                    MOVIEWAIT {
                        token: Some(self.token),
                        layer_id: self.layer_id,
                    }
                    .into(),
                )
            }
            _ => {
                warn!("MOVIEWAIT: layer is not a movie layer");
                self.token.finish().into()
            }
        }
    }
}

impl UpdatableCommand for MOVIEWAIT {
    fn update(
        &mut self,
        _context: &UpdateContext,
        _scenario: &Arc<Scenario>,
        vm_state: &VmState,
        adv_state: &mut AdvState,
        _is_fast_forwarding: bool,
    ) -> Option<CommandResult> {
        let UserLayer::MovieLayer(layer) = adv_state.get_layer(vm_state, self.layer_id).unwrap()
        else {
            unreachable!()
        };
        let finished = layer.is_finished();
        if finished {
            Some(self.token.take().unwrap().finish())
        } else {
            None
        }
    }
}

impl Debug for MOVIEWAIT {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_tuple("MOVIEWAIT").field(&self.layer_id).finish()
    }
}

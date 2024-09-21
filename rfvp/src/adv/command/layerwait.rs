use std::fmt::{Debug, Formatter};

use rfvp_core::{format::scenario::types::SMALL_LIST_SIZE, vm::command::types::LayerProperty};
use smallvec::SmallVec;

use super::prelude::*;

pub struct LAYERWAIT {
    layer_id: VLayerId,
    properties: SmallVec<LayerProperty, { SMALL_LIST_SIZE }>,
    token: Option<command::token::LAYERWAIT>,
}

impl StartableCommand for command::runtime::LAYERWAIT {
    fn apply_state(&self, _state: &mut VmState) {
        // the does not exist in the VmState, no need to wait
    }

    fn start(
        self,
        _context: &UpdateContext,
        _scenario: &Arc<Scenario>,
        _vm_state: &VmState,
        _adv_state: &mut AdvState,
    ) -> CommandStartResult {
        Yield(
            LAYERWAIT {
                layer_id: self.layer_id,
                properties: self.wait_properties,
                token: Some(self.token),
            }
            .into(),
        )
    }
}

impl UpdatableCommand for LAYERWAIT {
    fn update(
        &mut self,
        _context: &UpdateContext,
        _scenario: &Arc<Scenario>,
        vm_state: &VmState,
        adv_state: &mut AdvState,
        is_fast_forwarding: bool,
    ) -> Option<CommandResult> {
        if adv_state
            .get_vlayer_mut(vm_state, self.layer_id)
            .all(|mut l| {
                self.properties.iter().all(|&prop_id| {
                    let prop = l.properties_mut().property_tweener_mut(prop_id);
                    if is_fast_forwarding {
                        prop.fast_forward();
                    }
                    prop.is_idle()
                })
            })
        {
            Some(self.token.take().unwrap().finish())
        } else {
            None
        }
    }
}

impl Debug for LAYERWAIT {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        f.debug_tuple("LAYERWAIT")
            .field(&self.layer_id)
            .field(&self.properties)
            .finish()
    }
}

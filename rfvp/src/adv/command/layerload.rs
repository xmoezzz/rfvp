use std::fmt::{Debug, Formatter};

use pollster::FutureExt;
use rfvp_tasks::{AsyncComputeTaskPool, Task};

use super::prelude::*;
use crate::layer::UserLayer;

pub struct LAYERLOAD {
    token: Option<command::token::LAYERLOAD>,
    layer_id: VLayerId,
    load_task: Option<Task<UserLayer>>,
}

impl StartableCommand for command::runtime::LAYERLOAD {
    fn apply_state(&self, state: &mut VmState) {
        assert_eq!(self.leave_uninitialized, 0); // I __think__ this has to do with init props/leave them be, but I'm not sure

        match self.layer_id.repr() {
            VLayerIdRepr::RootLayerGroup
            | VLayerIdRepr::ScreenLayer
            | VLayerIdRepr::PageLayer
            | VLayerIdRepr::PlaneLayerGroup => {
                unreachable!("You can't load special layers")
            }
            VLayerIdRepr::Selected => {
                todo!("LAYERLOAD: selected");
            }
            VLayerIdRepr::Layer(id) => {
                // unwrap_or_else is unusable because of borrow checker
                let layer = match state.layers.get_layer_mut(id) {
                    None => state.layers.alloc(id),
                    Some(v) => v,
                };

                layer.layerinit_params = Some((self.layer_type, self.params));
            }
        }
    }

    fn start(
        self,
        context: &UpdateContext,
        scenario: &Arc<Scenario>,
        _vm_state: &VmState,
        adv_state: &mut AdvState,
    ) -> CommandStartResult {
        // TODO: loading should be done async
        let resources = context.gpu_resources.clone();
        let asset_server = context.asset_server.clone();
        let audio_manager = adv_state.audio_manager.clone();
        let scenario = scenario.clone();

        let load_task = AsyncComputeTaskPool::get().spawn(async move {
            UserLayer::load(
                &resources,
                &asset_server,
                &audio_manager,
                &scenario,
                self.layer_type,
                self.params,
            )
            .await
        });

        Yield(
            LAYERLOAD {
                token: Some(self.token),
                layer_id: self.layer_id,
                load_task: Some(load_task),
            }
            .into(),
        )
    }
}

impl UpdatableCommand for LAYERLOAD {
    fn update(
        &mut self,
        _context: &UpdateContext,
        _scenario: &Arc<Scenario>,
        vm_state: &VmState,
        adv_state: &mut AdvState,
        _is_fast_forwarding: bool,
    ) -> Option<CommandResult> {
        if self.load_task.as_ref().unwrap().is_finished() {
            let layer = self.load_task.take().unwrap().block_on();

            match self.layer_id.repr() {
                VLayerIdRepr::RootLayerGroup
                | VLayerIdRepr::ScreenLayer
                | VLayerIdRepr::PageLayer
                | VLayerIdRepr::PlaneLayerGroup => {
                    panic!("You can't load special layers")
                }
                VLayerIdRepr::Selected => {
                    todo!("LAYERLOAD: selected");
                }
                VLayerIdRepr::Layer(id) => adv_state
                    .current_plane_layer_group_mut(vm_state)
                    .add_layer(id, layer),
            }

            return Some(self.token.take().unwrap().finish());
        }

        None
    }
}

impl Debug for LAYERLOAD {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        f.debug_tuple("LAYERLOAD").field(&self.layer_id).finish()
    }
}

use rfvp_core::time::{Easing, Tween};

use super::prelude::*;

impl StartableCommand for command::runtime::LAYERCTRL {
    fn apply_state(&self, state: &mut VmState) {
        let (target_value, _time, _flags, _easing_param, ..) = self.params;

        state
            .layers
            .get_vlayer_mut(self.layer_id)
            .for_each(|layer| {
                layer
                    .properties
                    .set_property(self.property_id, target_value);
            });
    }

    fn start(
        self,
        _context: &UpdateContext,
        _scenario: &Arc<Scenario>,
        vm_state: &VmState,
        adv_state: &mut AdvState,
    ) -> CommandStartResult {
        let (target_value, duration, flags, easing_param, ..) = self.params;

        if flags.unused_1() != 0 || flags.unused_2() != 0 || flags.unused_3() != 0 {
            panic!("LAYERCTRL: unused flags are set: {:?}", flags);
        }

        if flags.scale_time() {
            warn!("LAYERCTRL: scale_time is set, but not supported");
        }
        if flags.delta() {
            // note: delta flag has a non-trivial interaction with queue clear flags
            warn!("LAYERCTRL: delta is set, but not supported");
        }
        if flags.ff_to_current() && flags.ff_to_target() {
            panic!("LAYERCTRL: both ff_to_current and ff_to_target flags are set");
        }
        if flags.prohibit_fast_forward() {
            warn!("LAYERCTRL: prohibit_fast_forwward is set, but not supported");
        }
        if flags.ignore_wait() {
            warn!("LAYERCTRL: ignore_wait is set, but not supported");
        }

        let easing = match flags.easing() {
            0 => Easing::Linear,
            1 => Easing::SineIn,
            2 => Easing::SineOut,
            3 => Easing::SineInOut,
            4 => Easing::Jump,
            5 => Easing::Power(easing_param),
            _ => panic!("LAYERCTRL: unknown easing function: {}", flags.easing()),
        };

        let mut changed = false;
        adv_state.get_vlayer_mut(vm_state, self.layer_id).for_each(|mut layer| {
            let tweener = layer
                .properties_mut()
                .property_tweener_mut(self.property_id);

            let from_value = tweener.target_value();
            let to_value = target_value as f32;
            let mut duration = duration;

            if tweener.value() != to_value {
                changed = true;
            }

            if flags.scale_time() {
                // this flag makes "duration" actually mean change rate (in value per tick)
                let change = (to_value - from_value).abs();
                duration = Ticks::from_f32(change / duration.as_f32());
            }

            if flags.ff_to_current() {
                if flags.delta() {
                    todo!("LAYERCTRL: ff_to_current and delta flags have an interaction that is not yet implemented");
                }

                let current = tweener.value();
                tweener.fast_forward_to(current);
            }
            if flags.ff_to_target() {
                tweener.fast_forward();
            }

            tweener.enqueue(target_value as f32, Tween { duration, easing })
        });

        if !self.property_id.is_implemented() && changed {
            warn!(
                "LAYERCTRL: property is not implemented yet (property_id={:?}, value={})",
                self.property_id, target_value
            );
        }

        self.token.finish().into()
    }
}

use enum_map::{enum_map, Enum};

use crate::input::{
    inputs::{KeyCode, MouseButton},
    Action, ActionMap, InputSet,
};

// TODO: move actions from here when an adequate derive macro will be available

/// Actions available in all ADV contexts
#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash, Enum)]
pub enum AdvMessageAction {
    Advance,
    HoldFastForward,
    Backlog,
    Rollback,
}

impl Action for AdvMessageAction {
    fn default_action_map() -> ActionMap<Self> {
        fn map(v: AdvMessageAction) -> InputSet {
            match v {
                AdvMessageAction::Advance => [
                    MouseButton::Left.into(),
                    KeyCode::Enter.into(),
                    KeyCode::Space.into(),
                ]
                .into_iter()
                .collect(),
                AdvMessageAction::HoldFastForward => {
                    [KeyCode::ControlLeft.into()].into_iter().collect()
                }
                AdvMessageAction::Backlog => [].into_iter().collect(),
                AdvMessageAction::Rollback => [].into_iter().collect(),
            }
        }

        ActionMap::new(enum_map! { v => map(v) })
    }
}

/// Overlay Manager actions
#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash, Enum)]
pub enum OverlayManagerAction {
    ToggleOverlayManager,
}

impl Action for OverlayManagerAction {
    fn default_action_map() -> ActionMap<Self> {
        fn map(v: OverlayManagerAction) -> InputSet {
            match v {
                OverlayManagerAction::ToggleOverlayManager => {
                    [KeyCode::F3.into()].into_iter().collect()
                }
            }
        }

        ActionMap::new(enum_map! { v => map(v) })
    }
}

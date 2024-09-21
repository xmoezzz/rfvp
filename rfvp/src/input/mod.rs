// use action_state::ActionState;
// use input_map::InputMap;
// use std::marker::PhantomData;

// pub mod action_state;
// pub mod axislike;
pub mod buttonlike;
// mod display_impl;
// pub mod errors;
// pub mod input_map;
// // pub mod plugin;
// // pub mod systems;
pub mod inputs;
// pub mod user_input;

// The Shiny New Input System
mod action;
pub mod actions;
mod raw_input_state;

pub use action::{Action, ActionMap, ActionState, InputSet, UserInput};
pub use raw_input_state::RawInputState;

// Importing the derive macro
// pub use leafwing_input_manager_macros::Actionlike;

// /// Everything you need to get started
// pub mod prelude {
//     pub use crate::input::action_state::ActionState;
//     pub use crate::input::axislike::{DualAxis, MouseWheelAxisType, SingleAxis, VirtualDPad};
//     pub use crate::input::buttonlike::MouseWheelDirection;
//     pub use crate::input::input_map::InputMap;
//     pub use crate::input::inputs::KeyCode;
//     pub use crate::input::user_input::{Modifier, UserInput};
// }

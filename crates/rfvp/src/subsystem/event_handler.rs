use winit::event::{MouseButton, WindowEvent};
use crate::subsystem::world::GameData;

use super::resources::input_manager::KeyCode;

pub fn update_input_events(window_event: &WindowEvent, data: &mut GameData) {
    match window_event {
        WindowEvent::KeyboardInput { event,.. } => {
            match event.state {
                winit::event::ElementState::Pressed => {
                    data.inputs_manager.notify_keydown(event.logical_key.clone(), event.repeat);
                }
                winit::event::ElementState::Released => {
                    data.inputs_manager.notify_keyup(event.logical_key.clone());
                }
            }
        }
        WindowEvent::MouseInput { state, button, .. } => {
            match state {
                winit::event::ElementState::Pressed => {
                    if *button == MouseButton::Left {
                        data.inputs_manager.notify_mouse_down(KeyCode::MouseLeft);
                    } else if *button == MouseButton::Right {
                        data.inputs_manager.notify_mouse_down(KeyCode::MouseRight);
                    }
                }
                winit::event::ElementState::Released => {
                    if *button == MouseButton::Left {
                        data.inputs_manager.notify_mouse_up(KeyCode::MouseLeft);
                    } else if *button == MouseButton::Right {
                        data.inputs_manager.notify_mouse_up(KeyCode::MouseRight);
                    }
                }
            }
        }
        WindowEvent::MouseWheel { delta, .. } => {
            match delta {
                winit::event::MouseScrollDelta::LineDelta(_, y) => {
                    data.inputs_manager.notify_mouse_wheel(*y as i32);
                }
                winit::event::MouseScrollDelta::PixelDelta(_) => {}
            }
        }
        WindowEvent::CursorMoved { position, .. } => {
            data.inputs_manager.notify_mouse_move(position.x as i32, position.y as i32);
        }
        WindowEvent::CursorEntered {..} => {
            data.inputs_manager.set_mouse_in(true);
        }
        WindowEvent::CursorLeft {..} => {
            data.inputs_manager.set_mouse_in(false);
        }
        WindowEvent::Focused(focused) => {
            // Original engine flushes input states on WM_ACTIVATEAPP.
            // Without this, we can keep stale pressed bits when focus transitions happen
            // (including the initial activation), which leads to unintended auto-click/skip.
            data.inputs_manager.set_flash();
            if *focused {
                // Eat the activation click (common on some platforms / backends).
                data.inputs_manager.suppress_next_mouse_click();
            }
            if !*focused {
                data.inputs_manager.set_mouse_in(false);
            }
        }
        _ => {}
    };
}

use winit::event::{MouseButton, WindowEvent};
use crate::subsystem::world::GameData;

use super::resources::input_manager::KeyCode;

/// Update the engine input state from a winit [`WindowEvent`].
///
/// Winit reports cursor positions in *physical pixels* of the window. The original
/// engine, however, uses the game's virtual resolution (the script/game coordinate
/// space) for cursor hit-tests and input syscalls.
///
/// Because we present the virtual render target into the window using an
/// aspect-ratio-preserving scale + letterboxing (see `App::render_frame` present
/// pass), we must apply the inverse transform here so that `cursor_x/cursor_y`
/// match the coordinate space expected by `PrimHit` and all input syscalls.
pub fn update_input_events(
    window_event: &WindowEvent,
    data: &mut GameData,
    surface_size: (u32, u32),
    virtual_size: (u32, u32),
) {
    fn map_physical_to_virtual(
        render_flag: i32,
        surface_size: (u32, u32),
        virtual_size: (u32, u32),
        px: f64,
        py: f64,
    ) -> (i32, i32, bool) {
        let (sw_u, sh_u) = surface_size;
        let (vw_u, vh_u) = virtual_size;

        let sw = sw_u.max(1) as f64;
        let sh = sh_u.max(1) as f64;
        let vw = vw_u.max(1) as f64;
        let vh = vh_u.max(1) as f64;

        if render_flag == 2 {
            // Stretch: window maps directly to the full virtual space.
            let mut vx = (px * vw / sw) as i32;
            let mut vy = (py * vh / sh) as i32;
            let max_x = (vw as i32).saturating_sub(1);
            let max_y = (vh as i32).saturating_sub(1);
            vx = vx.clamp(0, max_x);
            vy = vy.clamp(0, max_y);
            return (vx, vy, true);
        }

        // Keep-aspect letterbox.
        let scale = (sw / vw).min(sh / vh);
        let dst_w = vw * scale;
        let dst_h = vh * scale;
        let off_x = (sw - dst_w) * 0.5;
        let off_y = (sh - dst_h) * 0.5;

        let in_content = px >= off_x && px < (off_x + dst_w) && py >= off_y && py < (off_y + dst_h);

        let mut vx = ((px - off_x) / scale) as i32;
        let mut vy = ((py - off_y) / scale) as i32;
        if in_content {
            let max_x = (vw as i32).saturating_sub(1);
            let max_y = (vh as i32).saturating_sub(1);
            vx = vx.clamp(0, max_x);
            vy = vy.clamp(0, max_y);
        }
        (vx, vy, in_content)
    }

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
            // Keep InputGetState/InputGetDown/InputGetUp usable even when the VM is
            // advanced by an input signal (i.e. before the next frame boundary).
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
            let render_flag = data.get_render_flag();
            let (vx, vy, in_screen) = map_physical_to_virtual(
                render_flag,
                surface_size,
                virtual_size,
                position.x,
                position.y,
            );

            data.inputs_manager.notify_mouse_move(vx, vy);
            data.inputs_manager.set_mouse_in(in_screen);
        }
        WindowEvent::Touch(t) => {
            // Mobile touch -> emulate MouseLeft only.
            // Winit touch coordinates are in window physical pixels, same space as CursorMoved.
            let render_flag = data.get_render_flag();
            let (vx, vy, in_screen) = map_physical_to_virtual(
                render_flag,
                surface_size,
                virtual_size,
                t.location.x,
                t.location.y,
            );

            data.inputs_manager
                .notify_touch(t.phase, t.id, vx, vy, in_screen);
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

                // IMPORTANT: do not leave cursor_in stuck at false after a focus regain.
                // Some platforms do not emit CursorMoved/Entered on focus transitions.
                // The original engine's hit-testing logic keeps working as long as the
                // cursor is still inside the client area.
                data.inputs_manager.set_mouse_in(true);
            }
        }
        _ => {}
    };
}

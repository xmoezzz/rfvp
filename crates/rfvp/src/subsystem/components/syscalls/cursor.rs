use anyhow::Result;

use crate::script::Variant;
use crate::subsystem::world::GameData;

use super::{get_var, Syscaller};

/// CursorShow(show: int)
/// IDA SYSCALL_SPECS: argc=1
pub struct CursorShow;
impl Syscaller for CursorShow {
    fn call(&self, game_data: &mut GameData, args: Vec<Variant>) -> Result<Variant> {
        let show = !get_var!(args, 0).is_nil();
        // winit cursor show/hide is handled at the window layer.
        // We keep the value for the engine's own logic, and let the platform backend consume it if needed.
        game_data.window_mut().set_cursor_visible(show);
        Ok(Variant::Nil)
    }
}

/// CursorMove(x: int, y: int, mode: int)
/// IDA SYSCALL_SPECS: argc=3
pub struct CursorMove;
impl Syscaller for CursorMove {
    fn call(&self, game_data: &mut GameData, args: Vec<Variant>) -> Result<Variant> {
        let x = match get_var!(args, 0).as_int() {
            Some(v) => v,
            None => {
                return Err(anyhow::anyhow!(
                    "CursorMove: expected integer argument, got {:?}",
                    args[0]
                ))
            }
        };
        let y = match get_var!(args, 1).as_int() {
            Some(v) => v,
            None => {
                return Err(anyhow::anyhow!(
                    "CursorMove: expected integer argument, got {:?}",
                    args[1]
                ))
            }
        };
        let _mode = match get_var!(args, 2).as_int() {
            Some(v) => v,
            None => {
                return Err(anyhow::anyhow!(
                    "CursorMove: expected integer argument, got {:?}",
                    args[2]
                ))
            }
        };
        game_data.window_mut().set_cursor_pos(x, y);
        Ok(Variant::Nil)
    }
}

/// CursorChange(id: int)
/// IDA SYSCALL_SPECS: argc=1
pub struct CursorChange;
impl Syscaller for CursorChange {
    fn call(&self, game_data: &mut GameData, args: Vec<Variant>) -> Result<Variant> {
        let id = get_var!(args, 0).as_int().ok_or_else(|| {
            anyhow::anyhow!(
                "CursorChange: expected integer argument, got {:?}",
                args[0]
            )
        })?;
        // Engine cursor shape/theme. If your backend supports it, map id to a winit CursorIcon.
        game_data.window_mut().set_cursor_kind(id);
        Ok(Variant::Nil)
    }
}

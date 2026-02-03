use anyhow::Result;

use crate::script::Variant;
use crate::script::global::get_int_var;
use crate::subsystem::world::GameData;

use super::{get_var, Syscaller};

/// CursorShow(show: int)
/// IDA SYSCALL_SPECS: argc=1
pub struct CursorShow;
impl Syscaller for CursorShow {
    fn call(&self, game_data: &mut GameData, args: Vec<Variant>) -> Result<Variant> {
        let show = !get_var!(args, 0).is_nil();
        // Cursor visibility is handled at the window layer.
        // Keep the value for engine logic; the platform backend can consume it.
        game_data.window_mut().set_cursor_visible(show);
        Ok(Variant::Nil)
    }
}

/// CursorMove(x: int, y: int, force: any)
/// IDA SYSCALL_SPECS: argc=3
///
/// Original engine behavior (IDA decompilation):
/// - Only moves when args[0] and args[1] are integers.
/// - Movement is enabled when either:
///   (a) global int var (non_volatile_global_count + 15) equals 1, or
///   (b) args[2] is non-nil (force move).
pub struct CursorMove;
impl Syscaller for CursorMove {
    fn call(&self, game_data: &mut GameData, args: Vec<Variant>) -> Result<Variant> {
        let Some(x) = get_var!(args, 0).as_int() else {
            return Ok(Variant::Nil);
        };
        let Some(y) = get_var!(args, 1).as_int() else {
            return Ok(Variant::Nil);
        };

        let force = !get_var!(args, 2).is_nil();
        let allow = get_int_var(15) == 1;

        if allow || force {
            game_data.window_mut().set_cursor_pos(x, y);
        }

        Ok(Variant::Nil)
    }
}

/// CursorChange(id: int)
/// IDA SYSCALL_SPECS: argc=1
pub struct CursorChange;
impl Syscaller for CursorChange {
    fn call(&self, game_data: &mut GameData, args: Vec<Variant>) -> Result<Variant> {
        let Some(id) = get_var!(args, 0).as_int() else {
            return Ok(Variant::Nil);
        };
        if !(0..4).contains(&id) {
            return Ok(Variant::Nil);
        }
        // Engine cursor shape/theme. If your backend supports it, map id to a winit CursorIcon.
        game_data.window_mut().set_cursor_kind(id);
        Ok(Variant::Nil)
    }
}

unsafe impl Send for CursorShow {}
unsafe impl Sync for CursorShow {}
unsafe impl Send for CursorMove {}
unsafe impl Sync for CursorMove {}
unsafe impl Send for CursorChange {}
unsafe impl Sync for CursorChange {}

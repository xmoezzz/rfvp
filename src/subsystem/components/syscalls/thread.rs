use anyhow::Result;

use crate::script::Variant;
use crate::subsystem::resources::scripter::ScriptScheduler;
use crate::subsystem::world::GameData;

pub fn thread_exit(game_data: &mut GameData, id: i32) -> Result<Variant> {
    // scripter.thread_exit(id as u32)?;
    Ok(Variant::Nil)
}
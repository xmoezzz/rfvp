use crate::subsystem::world::GameData;
use crate::app::AppBuilder;

pub trait Package {
    fn prepare(&self, _data: &mut GameData) {}

    fn load(self, builder: AppBuilder) -> AppBuilder;
}

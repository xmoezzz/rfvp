use crate::app::AppBuilder;
use crate::subsystem::world::GameData;

pub trait Package {
    fn prepare(&self, _data: &mut GameData) {}

    fn load(self, builder: AppBuilder) -> AppBuilder;
}

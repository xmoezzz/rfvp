use crate::subsystem::components::material::Material;
use crate::subsystem::package::Package;

use crate::subsystem::resources::events::topic::TopicConfiguration;
use crate::subsystem::resources::events::Events;

use crate::subsystem::resources::time::{Time, TimerType, Timers};

use crate::subsystem::resources::asset_manager::AssetManager;
use crate::subsystem::resources::audio::Audio;
use crate::subsystem::resources::focus_manager::FocusManager;
use crate::subsystem::scene::SceneController;
use crate::subsystem::state::GameState;
use crate::subsystem::systems::default_camera_system::camera_dpi_system;
use crate::subsystem::systems::default_camera_system::default_camera_system;
use crate::subsystem::world::GameData;

use crate::app::AppBuilder;

pub(crate) mod default_camera_system;

pub(crate) struct InternalPackage;
impl Package for InternalPackage {
    fn prepare(&self, data: &mut GameData) {
        let mut events = Events::default();
        events
            .create_topic("Inputs", TopicConfiguration::default())
            .expect("Error while creating topic for inputs event");

        let mut timers = Timers::default();

        if cfg!(feature = "hot-reload") {
            let _res = timers.add_timer("hot-reload-timer", TimerType::Cyclic, 5.);
        }

        data.insert_resource(Time::default());
        data.insert_resource(FocusManager::default());
        data.insert_resource(events);
        data.insert_resource(timers);
        data.insert_resource(AssetManager::default());
        data.insert_resource(GameState::default());
        data.insert_resource(SceneController::default());
        data.insert_resource(Audio::default());
    }

    fn load(self, builder: AppBuilder) -> AppBuilder {
        builder
            .with_system(default_camera_system)
            .with_system(camera_dpi_system)
    }
}
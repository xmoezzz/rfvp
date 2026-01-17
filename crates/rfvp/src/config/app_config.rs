use serde::{Deserialize, Serialize};

use crate::config::{logger_config::LoggerConfig, window_config::WindowConfig};

/// Main configuration used to configure the game.
/// Please use [`AppConfigBuilder`] if you want to build if from code.
#[derive(Debug, Serialize, Deserialize)]
pub struct AppConfig {
    /// Name of the application
    pub(crate) app_name: String,
    /// Logger configuration to use.
    pub(crate) logger_config: Option<LoggerConfig>,
    /// Window configuration to use.
    pub(crate) window_config: Option<WindowConfig>,
}

impl Default for AppConfig {
    fn default() -> Self {
        Self {
            app_name: "FVP Game Engine".to_string(),
            logger_config: Some(Default::default()),
            window_config: Some(Default::default()),
        }
    }
}

/// `AppConfigBuilder` is a convenience builder to create a `AppConfig` from code.
pub struct AppConfigBuilder {
    config: AppConfig,
}

impl AppConfigBuilder {
    /// Create a new `AppConfigBuilder` builder
    pub fn new() -> Self {
        Self { config: Default::default() }
    }

    /// Sets the app name. Will also be used for the window name
    pub fn with_app_name(mut self, app_name: String) -> Self {
        self.config.app_name = app_name;
        self
    }

    /// Sets the logger configuration for the application
    pub fn with_logger_config(mut self, logger_config: LoggerConfig) -> Self {
        self.config.logger_config = Some(logger_config);
        self
    }

    /// Sets the main window configuration. `WindowConfig` can be built using `WindowConfigBuilder`
    pub fn with_window_config(mut self, window_config: WindowConfig) -> Self {
        self.config.window_config = Some(window_config);
        self
    }

    /// Retrieves the configuration built
    pub fn get(self) -> AppConfig {
        self.config
    }
}


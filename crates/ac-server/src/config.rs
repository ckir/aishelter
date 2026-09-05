use config::{Config, ConfigError, Environment, File};
use serde::Deserialize;

#[derive(Debug, Deserialize)]
pub struct Settings {
    pub database_url: String,
    pub host: String,
    pub port: u16,
}

impl Settings {
    pub fn new() -> Result<Self, ConfigError> {
        let builder = Config::builder()
            .add_source(File::new("config/default", config::FileFormat::Toml).required(false))
            .add_source(Environment::with_prefix("AC").separator("_"));

        builder.build()?.try_deserialize()
    }
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            database_url: "postgresql://localhost/aishelter".to_string(),
            host: "0.0.0.0".to_string(),
            port: 3000,
        }
    }
}

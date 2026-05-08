use crate::config::{Config, TomlConfig};
use crate::error::{Error, Result};
use std::path::Path;

pub struct ConfigLoader;

impl ConfigLoader {
    pub fn load_toml_file(path: &Path) -> Result<TomlConfig> {
        let content = std::fs::read_to_string(path)?;
        let config: TomlConfig = toml::from_str(&content)?;
        Ok(config)
    }

    pub fn load_yaml_file(path: &Path) -> Result<Config> {
        let content = std::fs::read_to_string(path)?;
        let config: Config = serde_yaml::from_str(&content)?;
        Ok(config)
    }

    pub fn load_default_toml() -> Result<Option<TomlConfig>> {
        let home = std::env::var("HOME").map_err(|_| {
            Error::Config("HOME environment variable not set".to_string())
        })?;
        let config_path = Path::new(&home)
            .join(".config")
            .join("tools-mcp")
            .join("config.toml");

        if config_path.exists() {
            Ok(Some(Self::load_toml_file(&config_path)?))
        } else {
            Ok(None)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::ServiceType;

    #[test]
    fn test_load_toml_config() {
        let toml_content = r#"
[profiles.test]
type = "mysql"
host = "localhost"
port = 3306
user = "root"
"#;
        let config: TomlConfig = toml::from_str(toml_content).unwrap();
        let profile = config.profiles.get("test").unwrap();
        assert_eq!(profile.service_type, ServiceType::Mysql);
        assert_eq!(profile.host.as_deref(), Some("localhost"));
        assert_eq!(profile.port, Some(3306));
    }
}

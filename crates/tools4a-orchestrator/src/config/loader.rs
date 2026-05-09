use crate::config::{Config, TomlConfig};
use std::path::Path;
use tools4a_core::{Error, Result};

pub struct ConfigLoader;

impl ConfigLoader {
    pub fn load_toml_file(path: &Path) -> Result<TomlConfig> {
        let content = std::fs::read_to_string(path).map_err(|e| {
            Error::Config(format!("cannot read TOML file '{}': {}", path.display(), e))
        })?;
        toml::from_str(&content)
            .map_err(|e| Error::Config(format!("invalid TOML in '{}': {}", path.display(), e)))
    }

    pub fn load_yaml_file(path: &Path) -> Result<Config> {
        let content = std::fs::read_to_string(path).map_err(|e| {
            Error::Config(format!("cannot read YAML file '{}': {}", path.display(), e))
        })?;
        serde_yml::from_str(&content)
            .map_err(|e| Error::Config(format!("invalid YAML in '{}': {}", path.display(), e)))
    }

    pub fn load_default_toml() -> Result<Option<TomlConfig>> {
        let config_dir = if let Ok(xdg) = std::env::var("XDG_CONFIG_HOME") {
            std::path::PathBuf::from(xdg)
        } else {
            let home = std::env::var("HOME")
                .map_err(|_| Error::Config("HOME environment variable not set".to_string()))?;
            std::path::PathBuf::from(home).join(".config")
        };
        let config_path = config_dir.join("tools4a").join("config.toml");

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
    use std::io::Write;
    use tempfile::NamedTempFile;

    #[test]
    fn test_load_toml_config() {
        let toml_content = r#"
[profiles.test]
type = "mysql"
host = "localhost"
port = 3306
user = "root"
"#;
        let mut tmp = NamedTempFile::new().unwrap();
        tmp.write_all(toml_content.as_bytes()).unwrap();

        let config = ConfigLoader::load_toml_file(tmp.path()).unwrap();
        let profile = config.profiles.get("test").unwrap();
        assert_eq!(profile.service_type, ServiceType::Mysql);
        assert_eq!(profile.host.as_deref(), Some("localhost"));
        assert_eq!(profile.port, Some(3306));
    }
}

use std::fs;
use tempfile::TempDir;
use tools4a_orchestrator::config::{ConfigLoader, ServiceType};

#[test]
fn test_load_yaml_config() {
    let temp_dir = TempDir::new().unwrap();
    let config_path = temp_dir.path().join("test.yaml");

    let yaml_content = r#"
type: mysql
host: localhost
port: 3306
user: root
password: secret
"#;

    fs::write(&config_path, yaml_content).unwrap();

    let config = ConfigLoader::load_yaml_file(&config_path).unwrap();
    assert_eq!(config.service_type, Some(ServiceType::Mysql));
    assert_eq!(config.host.as_deref(), Some("localhost"));
    assert_eq!(config.port, Some(3306));
    assert_eq!(config.user.as_deref(), Some("root"));
}

#[test]
fn test_load_toml_config() {
    let temp_dir = TempDir::new().unwrap();
    let config_path = temp_dir.path().join("test.toml");

    let toml_content = r#"
[profiles.test]
type = "mysql"
host = "localhost"
port = 3306
user = "root"
"#;

    fs::write(&config_path, toml_content).unwrap();

    let config = ConfigLoader::load_toml_file(&config_path).unwrap();
    let profile = config.profiles.get("test").unwrap();
    assert_eq!(profile.service_type, ServiceType::Mysql);
    assert_eq!(profile.host.as_deref(), Some("localhost"));
}

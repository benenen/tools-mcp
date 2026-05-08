use serde::{Deserialize, Serialize};
use std::str::FromStr;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ServiceType {
    Mysql,
    Redis,
    Ssh,
}

impl FromStr for ServiceType {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "mysql" => Ok(ServiceType::Mysql),
            "redis" => Ok(ServiceType::Redis),
            "ssh" => Ok(ServiceType::Ssh),
            _ => Err(format!("Invalid service type: {}", s)),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "lowercase")]
pub enum TunnelConfig {
    Direct,
    Ssh {
        /// One or more jump hosts in client→target order. YAML/TOML accepts
        /// either a single string (legacy single-hop) or a sequence of strings.
        #[serde(rename = "ssh_jump", deserialize_with = "deserialize_string_or_vec")]
        ssh_jumps: Vec<String>,
        ssh_user: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        ssh_password: Option<String>,
        #[serde(skip_serializing_if = "Option::is_none")]
        ssh_key_path: Option<String>,
        #[serde(default = "default_ssh_port")]
        ssh_port: u16,
    },
}

fn default_ssh_port() -> u16 {
    22
}

fn deserialize_string_or_vec<'de, D>(deserializer: D) -> std::result::Result<Vec<String>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    #[derive(Deserialize)]
    #[serde(untagged)]
    enum StringOrVec {
        String(String),
        Vec(Vec<String>),
    }
    match StringOrVec::deserialize(deserializer)? {
        StringOrVec::String(s) => Ok(vec![s]),
        StringOrVec::Vec(v) => Ok(v),
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Profile {
    #[serde(rename = "type")]
    pub service_type: ServiceType,
    pub host: Option<String>,
    pub port: Option<u16>,
    pub user: Option<String>,
    pub password: Option<String>,
    pub database: Option<String>,
    /// Redis database number. Ignored by non-Redis services.
    pub db: Option<u32>,
    pub key_path: Option<String>,
    pub tunnel: Option<TunnelConfig>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct TomlConfig {
    #[serde(default)]
    pub profiles: std::collections::HashMap<String, Profile>,
}

#[derive(Debug, Clone, Default, Deserialize)]
pub struct Config {
    #[serde(rename = "type")]
    pub service_type: Option<ServiceType>,
    pub host: Option<String>,
    pub port: Option<u16>,
    pub user: Option<String>,
    pub password: Option<String>,
    pub database: Option<String>,
    /// Redis database number. Ignored by non-Redis services.
    pub db: Option<u32>,
    pub key_path: Option<String>,
    pub tunnel: Option<TunnelConfig>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_service_type_from_str() {
        assert_eq!("mysql".parse::<ServiceType>().unwrap(), ServiceType::Mysql);
        assert_eq!("redis".parse::<ServiceType>().unwrap(), ServiceType::Redis);
        assert_eq!("ssh".parse::<ServiceType>().unwrap(), ServiceType::Ssh);
        assert!("invalid".parse::<ServiceType>().is_err());
    }

    #[test]
    fn test_tunnel_config_ssh_accepts_string_for_jump() {
        let yaml = r#"
type: ssh
ssh_jump: bastion.com
ssh_user: admin
"#;
        let cfg: TunnelConfig = serde_yml::from_str(yaml).unwrap();
        match cfg {
            TunnelConfig::Ssh {
                ssh_jumps,
                ssh_user,
                ..
            } => {
                assert_eq!(ssh_jumps, vec!["bastion.com".to_string()]);
                assert_eq!(ssh_user, "admin");
            }
            _ => panic!("expected Ssh"),
        }
    }

    #[test]
    fn test_tunnel_config_ssh_accepts_array_for_jump() {
        let yaml = r#"
type: ssh
ssh_jump:
  - bastion1.com
  - bastion2.com
ssh_user: admin
"#;
        let cfg: TunnelConfig = serde_yml::from_str(yaml).unwrap();
        match cfg {
            TunnelConfig::Ssh { ssh_jumps, .. } => {
                assert_eq!(
                    ssh_jumps,
                    vec!["bastion1.com".to_string(), "bastion2.com".to_string()]
                );
            }
            _ => panic!("expected Ssh"),
        }
    }
}

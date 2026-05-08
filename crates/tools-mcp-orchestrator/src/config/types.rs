use serde::{Deserialize, Serialize};
use std::str::FromStr;
use tools_mcp_core::TunnelConfig;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ServiceType {
    Mysql,
    Redis,
    Ssh,
    Http,
}

impl FromStr for ServiceType {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "mysql" => Ok(ServiceType::Mysql),
            "redis" => Ok(ServiceType::Redis),
            "ssh" => Ok(ServiceType::Ssh),
            "http" => Ok(ServiceType::Http),
            _ => Err(format!("Invalid service type: {}", s)),
        }
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
}

use crate::config::Config;

pub struct ConfigMerger;

impl ConfigMerger {
    pub fn merge(base: Config, override_cfg: Config) -> Config {
        Config {
            service_type: override_cfg.service_type.or(base.service_type),
            host: override_cfg.host.or(base.host),
            port: override_cfg.port.or(base.port),
            user: override_cfg.user.or(base.user),
            password: override_cfg.password.or(base.password),
            database: override_cfg.database.or(base.database),
            db: override_cfg.db.or(base.db),
            key_path: override_cfg.key_path.or(base.key_path),
            tunnel: override_cfg.tunnel.or(base.tunnel),
            timeout_secs: override_cfg.timeout_secs.or(base.timeout_secs),
        }
    }

    pub fn merge_multiple(configs: Vec<Config>) -> Config {
        configs.into_iter().fold(Config::default(), Self::merge)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::Config;

    #[test]
    fn test_merge_configs() {
        let base = Config {
            host: Some("base-host".to_string()),
            port: Some(3306),
            user: Some("base-user".to_string()),
            ..Default::default()
        };

        let override_cfg = Config {
            host: Some("override-host".to_string()),
            password: Some("override-pass".to_string()),
            ..Default::default()
        };

        let merged = ConfigMerger::merge(base, override_cfg);
        assert_eq!(merged.host.as_deref(), Some("override-host"));
        assert_eq!(merged.port, Some(3306));
        assert_eq!(merged.user.as_deref(), Some("base-user"));
        assert_eq!(merged.password.as_deref(), Some("override-pass"));
    }
}

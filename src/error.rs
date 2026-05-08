use std::fmt;

#[derive(Debug)]
pub enum Error {
    Config(String),
    Connection(String),
    Execution(String),
    Io(std::io::Error),
    Mysql(mysql_async::Error),
    Yaml(serde_yaml::Error),
    Toml(toml::de::Error),
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Error::Config(msg) => write!(f, "Configuration error: {}", msg),
            Error::Connection(msg) => write!(f, "Connection error: {}", msg),
            Error::Execution(msg) => write!(f, "Execution error: {}", msg),
            Error::Io(e) => write!(f, "IO error: {}", e),
            Error::Mysql(e) => write!(f, "MySQL error: {}", e),
            Error::Yaml(e) => write!(f, "YAML error: {}", e),
            Error::Toml(e) => write!(f, "TOML error: {}", e),
        }
    }
}

impl std::error::Error for Error {}

impl From<std::io::Error> for Error {
    fn from(e: std::io::Error) -> Self {
        Error::Io(e)
    }
}

impl From<mysql_async::Error> for Error {
    fn from(e: mysql_async::Error) -> Self {
        Error::Mysql(e)
    }
}

impl From<serde_yaml::Error> for Error {
    fn from(e: serde_yaml::Error) -> Self {
        Error::Yaml(e)
    }
}

impl From<toml::de::Error> for Error {
    fn from(e: toml::de::Error) -> Self {
        Error::Toml(e)
    }
}

pub type Result<T> = std::result::Result<T, Error>;

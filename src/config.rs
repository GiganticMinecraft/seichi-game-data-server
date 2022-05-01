use anyhow::Result;
use envy::Error;
use serde::Deserialize;

pub trait FromEnv: Sized {
    fn from_env() -> Result<Self, Error>;
}

trait FromEnvLikeKeyValuePairs: Sized {
    fn from_iter(iter: impl Iterator<Item = (String, String)> + Clone) -> Result<Self, Error>;
}

impl<T: FromEnvLikeKeyValuePairs> FromEnv for T {
    fn from_env() -> Result<Self, Error> {
        // std::env::Vars is !Clone
        Self::from_iter(std::env::vars().collect::<Vec<_>>().into_iter())
    }
}

#[derive(Deserialize, Debug)]
pub struct Config {
    pub source_database_config: SourceDatabaseConfig,
    pub http_config: HttpConfig,
}

impl FromEnvLikeKeyValuePairs for Config {
    fn from_iter(iter: impl Iterator<Item = (String, String)> + Clone) -> Result<Self, Error> {
        Ok(Self {
            source_database_config: SourceDatabaseConfig::from_iter(iter.clone())?,
            http_config: HttpConfig::from_iter(iter)?,
        })
    }
}

#[derive(Deserialize, Debug)]
pub struct SourceDatabaseConfig {
    pub host: String,
    pub port: Port,
    pub database_name: String,
    pub user: String,
    pub password: String,
}

impl FromEnvLikeKeyValuePairs for SourceDatabaseConfig {
    fn from_iter(iter: impl Iterator<Item = (String, String)>) -> Result<Self, Error> {
        envy::prefixed("DB_").from_iter(iter)
    }
}

#[allow(clippy::module_name_repetitions)]
#[derive(Deserialize, Debug)]
pub struct HttpConfig {
    pub host: String,
    pub port: Port,
}

#[derive(Deserialize, Eq, PartialEq, Debug)]
pub struct Port(pub u16);

impl FromEnvLikeKeyValuePairs for HttpConfig {
    fn from_iter(iter: impl Iterator<Item = (String, String)>) -> Result<Self, Error> {
        envy::prefixed("HTTP_").from_iter(iter)
    }
}

#[cfg(test)]
mod test {
    use crate::config::{Config, FromEnvLikeKeyValuePairs};

    #[test]
    fn read_config_from_iterator() {
        let setting = [
            ("HTTP_PORT".to_string(), "12345".to_string()),
            ("HTTP_HOST".to_string(), "127.0.0.1".to_string()),
            ("DB_HOST".to_string(), "example.com".to_string()),
            ("DB_PORT".to_string(), "3307".to_string()),
            ("DB_USER".to_string(), "bff".to_string()),
            ("DB_PASSWORD".to_string(), "$tr0ngpAssw0rd".to_string()),
        ];

        Config::from_iter(setting.into_iter()).unwrap();
    }
}

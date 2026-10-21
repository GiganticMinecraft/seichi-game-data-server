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
        // std::env::Vars is not Clone
        Self::from_iter(std::env::vars().collect::<Vec<_>>().into_iter())
    }
}

#[derive(Deserialize, Debug)]
pub struct AppConfig {
    pub source_database_config: SourceDatabaseConfig,
    pub http_config: HttpConfig,
}

impl FromEnvLikeKeyValuePairs for AppConfig {
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
    /// `DB_POOL_MAX_CONNECTIONS`: コネクションプールの最大接続数。
    ///
    /// 既定値 5 は従来のハードコード値と同じ。
    #[serde(default = "default_pool_max_connections")]
    pub pool_max_connections: u32,
    /// `DB_QUERY_TIMEOUT_SECS`: 1 クエリ (`fetch_all`) の制限時間 (秒)。
    ///
    /// どのクエリも `playerdata` を全件 SELECT するので、行数が増えても
    /// 正常系で打ち切られないよう既定値は 60 秒と長めにしている。
    /// gRPC サーバー側のリクエストタイムアウト (`main.rs` の
    /// `REQUEST_TIMEOUT`) より短くしておくこと。
    #[serde(default = "default_query_timeout_secs")]
    pub query_timeout_secs: u64,
}

const fn default_pool_max_connections() -> u32 {
    5
}

const fn default_query_timeout_secs() -> u64 {
    60
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
    use super::*;

    #[test]
    fn read_config_from_iterator() {
        let setting = [
            ("HTTP_PORT".to_string(), "12345".to_string()),
            ("HTTP_HOST".to_string(), "127.0.0.1".to_string()),
            ("DB_HOST".to_string(), "example.com".to_string()),
            ("DB_PORT".to_string(), "3307".to_string()),
            ("DB_DATABASE_NAME".to_string(), "db".to_string()),
            ("DB_USER".to_string(), "bff".to_string()),
            ("DB_PASSWORD".to_string(), "$tr0ngpAssw0rd".to_string()),
        ];

        let config = AppConfig::from_iter(setting.into_iter()).unwrap();

        assert_eq!(config.source_database_config.pool_max_connections, 5);
        assert_eq!(config.source_database_config.query_timeout_secs, 60);
    }

    #[test]
    fn read_db_timeouts_from_iterator() {
        let setting = [
            ("DB_HOST".to_string(), "example.com".to_string()),
            ("DB_PORT".to_string(), "3307".to_string()),
            ("DB_DATABASE_NAME".to_string(), "db".to_string()),
            ("DB_USER".to_string(), "bff".to_string()),
            ("DB_PASSWORD".to_string(), "$tr0ngpAssw0rd".to_string()),
            ("DB_POOL_MAX_CONNECTIONS".to_string(), "10".to_string()),
            ("DB_QUERY_TIMEOUT_SECS".to_string(), "2".to_string()),
        ];

        let config = SourceDatabaseConfig::from_iter(setting.into_iter()).unwrap();

        assert_eq!(config.pool_max_connections, 10);
        assert_eq!(config.query_timeout_secs, 2);
    }
}

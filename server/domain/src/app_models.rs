use anyhow::Result;
use async_trait::async_trait;
use std::fmt;
use std::time::Duration;

#[async_trait]
pub trait VecDataSource<T> {
    async fn fetch(&self) -> Result<Vec<T>>;
}

/// データソースへのクエリが制限時間内に終わらなかったことを表すエラー。
///
/// `VecDataSource::fetch` が返す `anyhow::Error` にこの型が入っている場合、
/// gRPC 層はそれを `DEADLINE_EXCEEDED` として呼び出し元に返す。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct QueryTimeoutError {
    pub timeout: Duration,
}

impl fmt::Display for QueryTimeoutError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "query did not finish within {:?}", self.timeout)
    }
}

impl std::error::Error for QueryTimeoutError {}

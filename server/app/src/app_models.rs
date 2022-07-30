use anyhow::Result;
use async_trait::async_trait;

#[async_trait]
pub trait VecDataSource<T> {
    async fn fetch(&self) -> Result<Vec<T>>;
}

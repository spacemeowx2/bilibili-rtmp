mod bilibili;

pub use std::collections::HashMap;
pub use async_trait::async_trait;
pub use bilibili::BilibiliService;

pub struct Authentication {
    url: String,
    key: Option<String>,
}

#[async_trait]
pub trait Service {
    async fn get_auth(params: &HashMap<String, String>) -> Result<Authentication, String>;
}

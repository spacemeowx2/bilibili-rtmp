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
    async fn get_auth(&self, params: &HashMap<String, String>) -> Result<Authentication, String>;
}
pub type ServiceMap = HashMap<String, Box<dyn Service + Send + Sync>>;

lazy_static! {
    pub static ref SERVICE_MAP: ServiceMap = {
        let mut m: ServiceMap = HashMap::new();
        m.insert("bilibili".to_string(), Box::new(BilibiliService::new()));
        m
    };
}

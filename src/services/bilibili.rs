use super::{async_trait, Authentication, Service};

pub struct BilibiliService {

}

impl BilibiliService {
    pub fn new() -> Self {
        Self {}
    }
}

#[async_trait]
impl Service for BilibiliService {
    async fn get_auth(&self, key: &str) -> Result<Authentication, String> {
        // the key is cookie right now
        Err(String::from("Not implemented"))
    }
}

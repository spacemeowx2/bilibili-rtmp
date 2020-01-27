use super::{async_trait, Authentication, Service, HashMap};

pub struct BilibiliService {

}

impl BilibiliService {
    pub fn new() -> Self {
        Self {}
    }
}

#[async_trait]
impl Service for BilibiliService {
    async fn get_auth(params: &HashMap<String, String>) -> Result<Authentication, String> {
        Err(String::from("Not implemented"))
    }
}

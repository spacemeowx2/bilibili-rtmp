use super::{async_trait, Authentication, Service};
use serde::{Serialize, Deserialize};
use reqwest::{header, Client};

#[derive(Deserialize)]
struct Response<T> {
    code: u32,
    message: String,
    data: T,
}


#[derive(Deserialize)]
struct Rtmp {
    addr: String,
    code: String,
}

#[derive(Deserialize)]
struct GetStreamByRoomId {
    rtmp: Rtmp,
}

pub struct BilibiliService {

}

impl BilibiliService {
    pub fn new() -> Self {
        Self {}
    }
}


impl BilibiliService {
    fn get_client(key: &str) -> Result<Client, Box<dyn std::error::Error>> {
        let mut headers = header::HeaderMap::new();
        headers.insert(header::COOKIE, header::HeaderValue::from_str(key)?);

        Ok(reqwest::Client::builder()
            .default_headers(headers)
            .build()?
        )
    }
    async fn get_auth_impl(&self, key: &str) -> Result<Authentication, Box<dyn std::error::Error>> {
        // the key is cookie right now
        let client = Self::get_client(key)?;
        let response = client.get("https://api.live.bilibili.com/live_stream/v1/StreamList/get_stream_by_roomId?room_id=930140")
            .send()
            .await?
            .json::<Response<GetStreamByRoomId>>()
            .await?;
        dbg!(response);

        unimplemented!();
    }
}

#[async_trait]
impl Service for BilibiliService {
    async fn get_auth(&self, key: &str) -> Result<Authentication, String> {
        self.get_auth_impl(key).await.map_err(|err| err.to_string())
    }
}

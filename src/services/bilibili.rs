use super::{async_trait, Authentication, Service};
use reqwest::{header, Client};
use types::*;

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
    async fn get_authentication(&self, client: &Client, room_id: String) -> Result<Authentication, Box<dyn std::error::Error>> {
        let url = format!("https://api.live.bilibili.com/live_stream/v1/StreamList/get_stream_by_roomId?room_id={}", room_id);
        let response = client.get(&url)
            .send()
            .await?
            .json::<Response<GetStreamByRoomId>>()
            .await?;
        let rtmp = match response.data {
            Some(GetStreamByRoomId { rtmp }) => rtmp,
            None => return Err(String::from("get_stream_by_roomId failed").into())
        };
        Ok(Authentication {
            url: rtmp.addr,
            key: Some(rtmp.code),
        })
    }
    async fn get_room_id(&self, client: &Client) -> Result<String, Box<dyn std::error::Error>> {
        let response = client.get("https://api.live.bilibili.com/live_user/v1/UserInfo/live_info")
            .send()
            .await?
            .json::<Response<LiveInfo>>()
            .await?;
        match response.data {
            Some(LiveInfo { roomid }) => Ok(roomid),
            None => Err(String::from("get_stream_by_roomId failed").into())
        }
    }
    async fn get_auth_impl(&self, key: &str) -> Result<Authentication, Box<dyn std::error::Error>> {
        // the key is cookie right now
        let client = Self::get_client(key)?;
        let room_id = self.get_room_id(&client).await?;
        self.get_authentication(&client, room_id).await
    }
}

#[async_trait]
impl Service for BilibiliService {
    async fn get_auth(&self, key: &str) -> Result<Authentication, String> {
        self.get_auth_impl(key).await.map_err(|err| err.to_string())
    }
}

mod types {
    use serde::Deserialize;

    #[derive(Deserialize, Debug)]
    pub struct Response<T> {
        pub code: i32,
        pub message: String,
        pub data: Option<T>,
    }


    #[derive(Deserialize, Debug)]
    pub struct Rtmp {
        pub addr: String,
        pub code: String,
    }

    #[derive(Deserialize, Debug)]
    pub struct GetStreamByRoomId {
        pub rtmp: Rtmp,
    }

    #[derive(Deserialize, Debug)]
    pub struct LiveInfo {
        pub roomid: String,
    }

}

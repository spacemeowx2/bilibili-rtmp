use headless_chrome::{Browser, LaunchOptionsBuilder};
use std::str::FromStr;
use headless_chrome::protocol::network::Cookie;

#[derive(Clone)]
pub struct LoginConfig {
    pub page_url: String,
    pub after_url: String,
    pub is_correct_login_check_fn: &'static (dyn Fn(&str) -> bool + Sync),
}

pub struct LoginBrowser {
    config: LoginConfig
}

#[derive(Debug, Clone)]
pub struct LoginResult {
    pub cookies: Vec<Cookie>,
}

impl LoginBrowser {
    pub fn new(config: LoginConfig) -> Self {
        LoginBrowser {
            config
        }
    }

    pub fn run(&self) -> Result<LoginResult, failure::Error> {
        // Create a new chrome browser that we can control
        let browser = Browser::new(
            LaunchOptionsBuilder::default()
                .headless(false)
                .build()
                .expect("Could not find chrome-executable")
        )?;

        let tab = browser.wait_for_initial_tab()?;
        tab.navigate_to(self.config.page_url.as_str())?;
        tab.wait_until_navigated()?;

        let config = self.config.clone();
        // tab.enable_request_interception(
        //     &[
        //         headless_chrome::protocol::network::methods::RequestPattern {
        //             url_pattern: Some(self.config.after_url.as_str()),
        //             resource_type: Some("XHR"),
        //             interception_stage: Some("HeadersReceived"),
        //         }
        //     ],
        //     Box::new(move |_transport, _session_id, event_params| {
        //         let post_data = event_params.request.post_data.unwrap();
        //         let headers = event_params.request.headers;

        //         let client = reqwest::Client::new();
        //         let mut header_map = reqwest::header::HeaderMap::new();
        //         for (key, value) in &headers {
        //             let header_name = HeaderName::from_str(key.as_str()).unwrap();
        //             let header_value = HeaderValue::from_str(value.as_str()).unwrap();
        //             header_map.append(header_name, header_value);
        //         }
        //         let mut response = client.post(config.login_post_url.as_str())
        //             .headers(header_map)
        //             .body(post_data)
        //             .send()
        //             .unwrap();
        //         let response_text = response.text().unwrap();
        //         let success = (config.is_correct_login_check_fn)(response_text.as_str());
        //         if success {
        //             let tx = tx_mutex.lock().unwrap();
        //             tx.send((
        //                 String::from(response_text),
        //                 headers,
        //             )).unwrap();
        //         }
        //         headless_chrome::browser::tab::RequestInterceptionDecision::Continue
        //     }),
        // )?;
        // let ret = rx.recv()?;
        loop {
            tab.wait_until_navigated()?;
            let url = tab.get_url();
            dbg!(&url);
            if url == config.after_url {
                break
            }
        }

        Ok(LoginResult {
            cookies: tab.get_cookies()?
        })
    }
}

use url::Url;

pub struct Client {
    client: reqwest::Client,
    base_url: Url,
}

impl Client {
    pub fn new(base_url: Url) -> Self {
        Self {
            client: reqwest::Client::new(),
            base_url,
        }
    }
}

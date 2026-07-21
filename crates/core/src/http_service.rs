use log::info;
use roadwork_sync::SyncData;
use serde::de::DeserializeOwned;
use std::collections::HashMap;

#[derive(Debug, Default)]
pub struct HttpService;

impl HttpService {
    pub async fn get_url(&self, url: &str) -> reqwest::Result<String> {
        info!("get_url {url}");
        reqwest::get(url).await?.text().await
    }

    pub async fn post_json_object<T: DeserializeOwned>(
        &self,
        url: &str,
        body: &HashMap<String, SyncData>,
        headers: &HashMap<String, String>,
    ) -> reqwest::Result<T> {
        info!("post_json_object");
        let client = reqwest::Client::new();
        let mut request_builder = client.request(reqwest::Method::POST, url);
        for header in headers {
            request_builder = request_builder.header(header.0, header.1);
        }
        request_builder.json(body).send().await?.json().await
    }
}

use log::info;
use roadwork_sync::SyncData;
use serde::Deserialize;
use serde::de::DeserializeOwned;
use std::collections::HashMap;

#[derive(Debug, Deserialize, Default)]
pub(crate) struct HttpService;

impl HttpService {
    pub(crate) fn get_url(&self, url: &str) -> reqwest::Result<String> {
        info!("get_url {url}");
        reqwest::blocking::get(url)?.text()
    }

    pub(crate) fn post_json_object<T: DeserializeOwned>(
        &self,
        url: &str,
        body: &HashMap<String, SyncData>,
        headers: &HashMap<String, String>,
    ) -> reqwest::Result<T> {
        info!("post_json_object");
        let client = reqwest::blocking::Client::new();
        let mut request_builder = client.request(reqwest::Method::POST, url);
        for header in headers {
            request_builder = request_builder.header(header.0, header.1);
        }

        request_builder.json(body).send()?.json()
    }
}

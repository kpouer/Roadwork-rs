use log::info;
use roadwork_sync::SyncData;
use serde::Deserialize;
use serde::de::DeserializeOwned;
use std::collections::HashMap;
use crate::MyError;
use crate::MyError::ReqwestERror;

#[derive(Debug, Deserialize, Default)]
pub(crate) struct HttpService;

impl HttpService {
    pub(crate) fn getUrl(&self, url: &str) -> Result<String, MyError> {
        info!("getUrl {url}");
        reqwest::blocking::get(url)
            .map_err(ReqwestERror)?
            .text()
            .map_err(ReqwestERror)
    }

    pub(crate) fn getJsonObject<T: DeserializeOwned>(url: &str) -> Result<T, MyError> {
        reqwest::blocking::get(url)
            .map_err(ReqwestERror)?
            .json()
            .map_err(ReqwestERror)
    }

    pub(crate) fn postJsonObject<T: DeserializeOwned>(
        &self,
        url: &str,
        body: &HashMap<String, SyncData>,
        headers: &HashMap<String, String>,
    ) -> Result<T, MyError> {
        info!("postJsonObject");
        let client = reqwest::blocking::Client::new();
        let mut request_builder = client.request(reqwest::Method::POST, url);
        for header in headers {
            request_builder = request_builder.header(header.0, header.1);
        }

        request_builder
            .json(body)
            .send()
            .map_err(ReqwestERror)?
            .json()
            .map_err(ReqwestERror)
    }
}

use crate::serialization;

#[derive(Debug, serde::Serialize, serde::Deserialize)]
pub enum ClientRequest {
    Connect(String),
    Disconnect,
}

#[derive(Debug, serde::Serialize, serde::Deserialize)]
pub enum ServerPush {
    ServerInfo(ServerInfo),
    Disconnect,
}

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
pub struct ServerInfo {
    pub main_package_name: String,
    pub content_base_url: String,
    pub version: String,
    pub external_components: serialization::FailableDeserialization<Vec<String>>,   // not really
}

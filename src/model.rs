use uuid::Uuid;
use ham_rs::rig::Mode;

#[derive(Debug, Serialize, Deserialize)]
pub struct Receiver {
    #[serde(rename = "ID")] 
    pub id: Uuid,
    #[serde(rename = "Mode")] 
    pub mode: Mode,
    #[serde(rename = "Frequency")] 
    pub frequency: f32,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct Radio {
    #[serde(rename = "ID")]
    pub id: Uuid,
    #[serde(rename = "Name")]
    pub name: String
}

#[derive(Debug, Serialize, Deserialize)]
pub struct Version {
    #[serde(rename = "ProtocolVersion")]
    pub protocol_version: String,
    #[serde(rename = "Host")]
    pub host: String,
    #[serde(rename = "HostVersion")]
    pub host_version: String,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(tag = "cmd")]
pub enum CommandResponse {
    #[serde(rename = "getReceiversResponse")]
    Receivers{ Receivers: Vec<Receiver> },
    #[serde(rename = "getVersionResponse")]
    Version(Version),
    #[serde(rename = "getRadiosResponse")]
    Radios{ Radios: Vec<Radio> }
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(tag = "cmd")]
pub enum Command {
    #[serde(rename = "getReceivers")]
    GetReceivers,
    #[serde(rename = "setFrequency")]
    SetFrequency{ Frequency: String, ID: Uuid },
    #[serde(rename = "setMode")]
    SetMode{ Mode: Mode, ID: Uuid },
    #[serde(rename = "getVersion")]
    GetVersion,
    #[serde(rename = "getRadios")]
    GetRadios,
    #[serde(rename = "addReceiver")]
    AddReceiver{ ID: Uuid }
}
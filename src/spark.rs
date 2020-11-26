use serde::{Deserialize, Deserializer};
use ham_rs::{Call,Grid};
use ham_rs::rig::{Mode};
use chrono::prelude::*;

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct Receiver {
    #[serde(rename = "ID")] 
    pub id: u32,
    #[serde(rename = "Mode")] 
    pub mode: Mode,
    #[serde(rename = "Frequency")] 
    pub frequency: f32,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct Radio {
    #[serde(rename = "ID")]
    pub id: u32,
    #[serde(rename = "Name")]
    pub name: String,
    #[serde(rename = "Running")]
    pub running: bool
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct Version {
    #[serde(rename = "ProtocolVersion")]
    pub protocol_version: String,
    #[serde(rename = "Host")]
    pub host: String,
    #[serde(rename = "HostVersion")]
    pub host_version: String,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct Spot {
    pub time: DateTime<Utc>,
    pub frequency: f32,
    #[serde(rename = "tunedfrequency")]
    pub tuned_frequency: f32,
    pub power: i32,
    pub drift: i32,
    pub snr: i32,
    pub dt: f32,
    pub msg: String,
    pub mode: Mode,
    pub distance: Option<f32>,
    #[serde(deserialize_with = "callsign_as_string")]
    pub call: Call,
    pub color: i32,
    pub locator: Option<Grid>,
    pub valid: bool
}

pub fn callsign_as_string<'de, D>(deserializer: D) -> Result<Call, D::Error>
    where D: Deserializer<'de>
{
    let v : String = Deserialize::deserialize(deserializer)?;
    Ok(Call::new(v))
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(tag = "cmd")]
pub enum CommandResponse {
    #[serde(rename = "getReceiversResponse")]
    Receivers{
        #[serde(rename = "Receivers")]
        receivers: Vec<Receiver> 
    },
    #[serde(rename = "getVersionResponse")]
    Version(Version),
    #[serde(rename = "getRadiosResponse")]
    Radios{
        #[serde(rename = "Radios")]
        radios: Vec<Radio> 
    },
    #[serde(rename = "spotResponse")]
    Spots{ 
        spots: Vec<Spot>
    },
    #[serde(rename = "ReceiverResponse")]
    ReceiverResponse{
        #[serde(rename = "ID")]
        id: u32,
        #[serde(rename = "Mode")]
        mode: Mode,
        #[serde(rename = "Frequency")]
        frequency: f32
    }
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(tag = "cmd")]
pub enum Command {
    #[serde(rename = "getReceivers")]
    GetReceivers,
    #[serde(rename = "setFrequency")]
    SetFrequency{ 
        #[serde(rename = "Frequency")]
        frequency: String,
        #[serde(rename = "ID")]
        id: u32
    },
    #[serde(rename = "setMode")]
    SetMode{
        #[serde(rename = "Mode")]
        mode: Mode,
        #[serde(rename = "ID")]
        id: u32
    },
    #[serde(rename = "getVersion")]
    GetVersion,
    #[serde(rename = "getRadios")]
    GetRadios,
    #[serde(rename = "addReceiver")]
    AddReceiver{ 
        #[serde(rename = "ID")]
        id: u32
    },
    #[serde(rename = "removeReceiver")]
    RemoveReceiver{
        #[serde(rename = "ID")]
        id: u32 
    },
    #[serde(rename = "setRunning")]
    SetRunning{
        #[serde(rename = "ID")]
        id: u32,
        #[serde(rename = "Running")]
        running: bool
    },
    #[serde(rename = "subscribeToSpots")]
    SubscribeToSpots{
        #[serde(rename = "Enable")]
        enable: bool
    },
    #[serde(rename = "subscribeToAudio")]
    SubscribeToAudio{ 
        #[serde(rename = "RxID")]
        rx_id: u32,
        #[serde(rename = "Enable")]
        enable: bool
    },
    #[serde(rename = "subscribeToSpectrum")]
    SubscribeToSpectrum{
        #[serde(rename = "RxID")]
        rx_id: u32,
        #[serde(rename = "Enable")]
        enable: bool
    }
}
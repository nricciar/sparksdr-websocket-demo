use serde::{Deserialize};
use ham_rs::rig::{Mode};
use crate::spot::Spot;

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct Receiver {
    #[serde(rename = "ID")] 
    pub id: u32,
    #[serde(rename = "Mode")] 
    pub mode: Mode,
    #[serde(rename = "Frequency")] 
    pub frequency: f32,
}

impl Receiver {
    pub fn has_spots(&self) -> bool {
        match self.mode {
            Mode::FT8 | Mode::FT4 | Mode::JT65 | Mode::JT9 | Mode::WSPR => true,
            _ => false,
        }
    }
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
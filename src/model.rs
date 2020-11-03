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
#[serde(tag = "cmd")]
pub enum CommandResponse {
    #[serde(rename = "getReceiversResponse")]
    Receivers{ Receivers: Vec<Receiver> } 
}

#[derive(Debug, Serialize, Deserialize)]
pub enum Command {
    getReceivers,
    setFrequency(f32),
    setMode(Mode)
}

#[derive(Debug, Serialize, Deserialize)]
pub struct CommandMessage {
    pub cmd: Command,
    pub receiver: Option<Uuid>,
}
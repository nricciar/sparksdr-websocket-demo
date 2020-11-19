use anyhow::Error;
use yew::prelude::*;
use yew::services::{ConsoleService,Task};
use yew::{html, ComponentLink, Html};
use yew_router::{route::Route, service::RouteService};
use yew_router::{Switch};
use yew::services::fetch::{FetchTask};
use yew::services::reader::{File, FileData, ReaderService, ReaderTask};
use web_sys::{WebSocket};
use uuid::Uuid;
use std::str;
use web_sys::{AudioContext, AudioBuffer, AudioBufferSourceNode};
use std::cell::RefCell;
use std::rc::Rc;

use ham_rs::Call;
use ham_rs::countries::{CountryInfo};
use ham_rs::rig::{Receiver,Radio,Version,Command,CommandResponse,RECEIVER_MODES,Mode,Spot};
use ham_rs::log::LogEntry;

pub struct Model {
    // Currently unused
    pub route_service: RouteService<()>,
    pub route: Route<()>,
    // Callbacks
    pub link: ComponentLink<Self>,
    // Console logging
    pub console: ConsoleService,
    // SparkSDR connection
    pub wss: WebSocket,
    // List of receivers from getReceivers command
    pub receivers: Vec<Receiver>,
    // List of radios from the getRadios command
    pub radios: Vec<Radio>,
    // Currently selected receiver
    pub default_receiver: Option<Uuid>,
    // Version response from the getVersion command
    pub version: Option<Version>,
    // TODO: temporary to keep local ui updated
    pub poll: Option<Box<dyn Task>>,
    // Spots from enabling SubscribeToSpots
    pub spots: Vec<Spot>,
    // Show/Hide receiver list
    pub show_receiver_list: bool,
    // Imported log file (ADIF format) for spot cross checking
    pub import: Option<Vec<LogEntry>>,
    // Services for file importing (log file)
    pub reader: ReaderService,
    pub tasks: Vec<ReaderTask>,
    // Local callsign cache
    pub callsigns: Vec<CallsignInfo>,
    // audio playback
    pub audio_ctx: AudioContext,
    pub source: AudioBufferSourceNode,
    pub buffer: Rc<RefCell<Option<AudioBuffer>>>,
}

// Currently this is unused as there is only one route: /
#[derive(Clone,Switch, Debug)]
pub enum AppRoute {
    #[to = "/"]
    Index,
}

// Currently only TextMsg is implemented and this is the
// commands to and responses from SparkSDR.
// BinaryMsg would be for future support for audio in/out
pub enum WebsocketMsgType {
    BinaryMsg(js_sys::ArrayBuffer),
    TextMsg(String)
}

type Chunks = bool;

// Used with the local callsign cache for our requests
// for callsign info.
pub enum CallsignInfo {
    Requested((Call, FetchTask)),
    Found(Call),
    NotFound(Call)
}

impl CallsignInfo {
    pub fn call(&self) -> Call {
        match self {
            CallsignInfo::Requested((c, _)) => c.clone(),
            CallsignInfo::Found(c) => c.clone(),
            CallsignInfo::NotFound(c) => c.clone(),
        }
    }
}

pub enum Msg {
    // not implemented
    RouteChanged(Route<()>),
    ChangeRoute(AppRoute),
    // websocket connection
    Disconnected,
    Connected,
    // Command responses from SparkSDR (e.g. getReceiversResponse, getVersionResponse)
    ReceivedText(Result<CommandResponse, Error>),
    // UI request frequency change up/down on digit X for receiver Uuid
    FrequencyUp(Uuid, i32), // digit 0 - 8 
    FrequencyDown(Uuid, i32), // digit 0 - 8
    // UI request to change receiver Uuid mode
    ModeChanged(Uuid, Mode),
    // UI request to set the default receiver to Uuid
    SetDefaultReceiver(Uuid),
    // UI request to add a receiver to radio Uuid
    AddReceiver(Uuid),
    // Not implemented (future support for audio data)
    ReceivedAudio(js_sys::ArrayBuffer),
    // UI toggle show/hide receiver list
    ToggleReceiverList,
    // None
    None,
    // TODO: poll tick (temporary)
    Tick,
    // File import (log file)
    Files(Vec<File>, Chunks),
    Loaded(FileData),
    CancelImport,
    ConfirmImport,
    // Response to our callsign info request
    CallsignInfoReady(Result<Call,Error>)
}

impl Model {
    pub fn toggle_receivers(&self) -> Html {
        let cls = if self.show_receiver_list == true {
            "fa-chevron-up"
        } else {
            "fa-chevron-down"
        };
        html! {
            <button class="button is-text" onclick=self.link.callback(move |_| Msg::ToggleReceiverList)>
                <span>{ format!("{} Receivers", self.receivers.len()) }</span>
                <span class="icon is-small">
                    <i class=("fas", cls)></i>
                </span>
            </button>
        }
    }
    pub fn spots_view(&self) -> Html {
        html! {
            <>
                <div style="clear:both"></div>

                <div class="s">
                    <table class="table">
                        <tr>
                            <th>{ "UTC" }</th>
                            <th>{ "dB" }</th>
                            <th>{ "DT" }</th>
                            <th>{ "Freq" }</th>
                            <th>{ "Mode" }</th>
                            <th>{ "Dist" }</th>
                            <th>{ "Message" }</th>
                            <th></th>
                            <th></th>
                            <th></th>
                        </tr>
                        { for self.spots.iter().rev().map(|s| {
                            self.spot(&s)
                          })
                        }
                    </table>
                </div>

                { self.import_adif_form() }
            </>
        }
    }

    fn import_adif_form(&self) -> Html {
        html! {
                <div class="import">
                    {
                        match &self.import {
                            None => html! {
                                <>
                    <p>{"Import log file (adif format) to cross check spots."}</p>
                    <input type="file" multiple=true onchange=self.link.callback(move |value| {
                            let mut result = Vec::new();
                            if let ChangeData::Files(files) = value {
                                let files = js_sys::try_iter(&files)
                                    .unwrap()
                                    .unwrap()
                                    .into_iter()
                                    .map(|v| File::from(v.unwrap()));
                                result.extend(files);
                            }
                            Msg::Files(result, false)
                        })/>
                                </>
                            },
                            Some(import) => html! {
                                <>
                                    <p>{ format!("Found {} records", import.len()) }</p>
                                    <p>
                                        <input type="button" class="button" value="Cancel Import" onclick=self.link.callback(|_| Msg::CancelImport) />
                                    </p>
                                </>
                            },
                        }
                    }
                </div>
        }
    }

    pub fn send_command(&mut self, cmd: Command) {
        let j = serde_json::to_string(&cmd).unwrap();
        let msg = format!("sent: {}", j);
        self.console.log(&msg);
        self.wss.send_with_str(&j).unwrap();
    }

    fn spot(&self, spot: &Spot) -> Html {
        let call = Call::new(spot.call.to_string());
        let (country_icon, state_class) =
            match call.country() {
                Ok(country) => {
                    let (new_country, new_state) =
                        match &self.import {
                            Some(import) => {
                                let new_country =
                                    if let Some(_index) = import.iter().position(|i| i.call.country() == Ok(country.clone())) {
                                        ""
                                    } else {
                                        "new-country"
                                    };
                                let new_state =
                                    match call.state() {
                                        Some(state) => {
                                            if let Some(_index) = import.iter().position(|i| i.call.state() == Some(state.to_string())) {
                                                ""
                                            } else {
                                                "new-state"
                                            }
                                        },
                                        _ => "",
                                    };
                                (new_country, new_state)
                            },
                            None => ("", ""),
                        };
                    (html! { <><i class=format!("flag-icon flag-icon-{}", country.code())></i> <span class=new_country>{ country.name() }</span></> }, new_state)
                },
                Err(_) => (html! {}, ""),
            };

        html! {
            <tr>
                <td>{ spot.time.format("%H%M%S") }</td>
                <td>{ spot.snr }</td>
                <td>{ spot.dt }</td>
                <td>{ format!("{} (+{})", spot.tuned_frequency, spot.frequency) }</td>
                <th>{ spot.mode.mode() }</th>
                <td>{ match spot.distance {
                         Some(dist) => format!("{}", dist),
                         None => format!(""),
                      }
                    }</td>
                {
                    match spot.msg.contains("CQ") {
                        true => html! { <th>{ spot.msg.to_string() }</th> },
                        false => html! { <td>{ spot.msg.to_string() }</td> }
                    }
                }
                <td>{ country_icon }</td>
                <td>{ match spot.call.state() {
                          Some(state) => format!("{}", state),
                          None => format!("")
                      } }</td>
                <td class=state_class>{ match spot.call.op() {
                          Some(op) => format!("{}", op),
                          None => format!("")
                      } }</td>
            </tr>
        }
    }

    pub fn radio(&self, radio: &Radio) -> Html {
        let radio_id = radio.id;
        html! {
            <div class="radio-control">
                <button class="button is-text" disabled=true>
                    { radio.name.to_string() }
                </button>
                <button class="button" disabled=true title="Power">
                    <span class="icon is-small">
                    <i class="fas fa-power-off fa-lg"></i>
                    </span>
                </button>
                <button class="button" onclick=self.link.callback(move |_| Msg::AddReceiver(radio_id) ) title="Add Receiver">
                    <span class="icon is-small">
                    <i class="fas fa-plus fa-lg"></i>
                    </span>
                </button>
            </div>
        }
    }

    pub fn receiver(&self, receiver: &Receiver) -> Html {
        let frequency_string = format!("{:0>9}", receiver.frequency.to_string());
        let tmp = self.decimal_mark(frequency_string);
        let mut inactive = true;
        let receiver_id = receiver.id;
        let class_name = 
            if Some(receiver.id) == self.default_receiver {
                "receiver-control selected"
            } else {
                "receiver-control"
            };

        html! {
            <form class=class_name onclick=self.link.callback(move |_| Msg::SetDefaultReceiver(receiver_id))>
                <div class="up-controls">
                    {
                        for (0..9).map(|digit| {
                            html! { <><a onclick=self.link.callback(move |_| Msg::FrequencyUp(receiver_id, digit))>{ "0" }</a>{ if digit == 2 || digit == 5 { "," } else { "" } }</> }
                        })
                    }
                </div>
                <div id="frequency" class="frequency">
                    {
                        for tmp.chars().map(|c| {
                            if ((c != '0' && c != ',' && inactive == true)) {
                                inactive = false;
                            }
                            match c {
                                ',' => html! { { "," } },
                                _ if inactive => html! { <span>{ c.to_string() }</span> },
                                _ => html! { <span class="active">{ c.to_string() }</span> }
                            }
                        })
                    }
                </div>
                <div class="down-controls">
                    {
                        for (0..9).map(|digit| {
                            html! { <><a onclick=self.link.callback(move |_| Msg::FrequencyDown(receiver_id, digit))>{ "0" }</a>{ if digit == 2 || digit == 5 { "," } else { "" } }</> }
                        })
                    }
                </div>
                <div class="mode control" style="margin-top:-0.5em;z-index:50">
                    <select id="mode" class="select" 
                        onchange=self.link.callback(move |e:ChangeData| 
                            match e {
                                ChangeData::Select(sel) => {
                                    Msg::ModeChanged(receiver_id, Mode::new(sel.value()))
                                },
                                _ => { Msg::None }
                            } )>
                        {
                            for RECEIVER_MODES.iter().map(|mode| {
                                html! { <option selected=if mode == &receiver.mode { true } else { false }>{ mode.mode() }</option> }
                            })
                        }
                    </select>
                </div>
            </form>
        }
    }

    fn decimal_mark(&self, s: String) -> String {
        let bytes: Vec<_> = s.bytes().rev().collect();
        let chunks: Vec<_> = bytes.chunks(3).map(|chunk| str::from_utf8(chunk).unwrap()).collect();
        let result: Vec<_> = chunks.join(",").bytes().rev().collect();
        String::from_utf8(result).unwrap()
    }
}
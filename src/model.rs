use anyhow::Error;
use yew::prelude::*;
use yew::services::{ConsoleService};
use yew::{html, ComponentLink, Html};
use yew_router::{route::Route, service::RouteService};
use yew_router::{Switch};
use yew::format::{Json};
use yew::services::reader::{File, FileData, ReaderService, ReaderTask};
use yew::services::websocket::{WebSocketStatus};
use yew::services::storage::{Area, StorageService};
use web_sys::{WebSocket,BinaryType,MessageEvent};
use std::str;
use wasm_bindgen::prelude::*;
use wasm_bindgen::JsCast;

use ham_rs::{Call,Country,CountryInfo,LogEntry,Mode};
use ham_rs::lotw::LoTWStatus;

use sparkplug::{Command,CommandResponse,Receiver,Radio,Version,RECEIVER_MODES,Spot};
use crate::spot::{SpotDB};
use crate::audio::{AudioProvider};
use crate::spectrum::{SpectrumProvider};

const LOGBOOK_KEY: &str = "radio.logs";

pub struct Model {
    pub route_service: RouteService<()>,
    pub route: Route<()>,
    storage: StorageService,
    // Callbacks
    pub link: ComponentLink<Self>,
    // SparkSDR connection
    pub ws_location: String,
    wss: Option<WebSocket>,

    // List of receivers from getReceivers command
    receivers: Vec<Receiver>,
    // List of radios from the getRadios command
    radios: Vec<Radio>,
    // Version response from the getVersion command
    version: Option<Version>,
    // Currently selected receiver
    default_receiver: Option<u32>,

    pub spots: SpotDB,
    pub audio: AudioProvider,
    pub spectrum: SpectrumProvider,

    // Show/Hide receiver list
    show_receiver_list: bool,
    // Imported log file (ADIF format) for spot cross checking
    pub import: Option<Vec<LogEntry>>,
    // Services for file importing (log file)
    reader: ReaderService,
    tasks: Vec<ReaderTask>,
}

// Currently this is unused as there is only one route: /
#[derive(Clone,Switch, Debug)]
pub enum AppRoute {
    #[to = "/map"]
    Map,
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

pub enum Msg {
    // Not implemented
    RouteChanged(Route<()>),
    ChangeRoute(AppRoute),

    // Websocket connection
    Connect,
    Disconnected,
    Connected,
    UpdateWebsocketAddress(String),

    // Command responses from SparkSDR (e.g. getReceiversResponse, getVersionResponse)
    CommandResponse(Result<CommandResponse, Error>),
    // Audio/Spectrum data
    ReceivedAudio(js_sys::ArrayBuffer),

    // The following Msg will result in commands
    // being sent to SparkSDR

    // Request change to receiver frequency
    FrequencyUp(u32, i32), // digit 0 - 8 
    FrequencyDown(u32, i32), // digit 0 - 8
    // Request change to receiver mode
    ModeChanged(u32, Mode),
    // Request to add a receiver to a radio
    AddReceiver(u32),
    // Request to remove a receiver
    RemoveReceiver(u32),
    // Toggle radio power state
    TogglePower(u32),
    // Request change to the default receiver
    // Will change audio subscription (if subscribed)
    // Will change spectrum subscription
    SetDefaultReceiver(u32),
    // Subscribe/Unsubscribe to default receivers audio channel
    EnableAudio,

    // Local only messages

    ToggleReceiverList,
    None,
    // Log file import (adif file format)
    Files(Vec<File>, Chunks),
    Loaded(FileData),
    CancelImport,
    ConfirmImport,
    // Control for client playback/volume
    SetGain(f32),
    MuteUnmute,
    ClearSpots,

    // Spot messages

    // Response to our callsign info request
    CallsignInfoReady(Result<Call,Error>),
    // Response to our LoTW users request
    LotwUsers(String),
    // States geoJson data
    StatesOverlay(String),
    // Set/Unset various spot filters
    ToggleCQSpotFilter,
    ToggleStateSpotFilter,
    ToggleCountrySpotFilter,
    ToggleCurrentReceiverSpotFilter,
    ToggleLoTWSpotFilter,
}

impl Model {
    pub fn new(link: ComponentLink<Self>) -> Model {
        let mut route_service: RouteService<()> = RouteService::new();
        let route = route_service.get_route();
        let callback = link.callback(Msg::RouteChanged);
        route_service.register_callback(callback);

        let storage = StorageService::new(Area::Local).expect("storage was disabled by the user");
        let entries = 
            match storage.restore(LOGBOOK_KEY) {
                Json(Ok(entries)) => {
                    ConsoleService::log("found log files");
                    entries
                },
                Json(Err(err)) => {
                    ConsoleService::error(&format!("log import error: {}", err));
                    None
                },
                _ => None
            };

        let spot_db = SpotDB::new();
        spot_db.update_states_overlay_js();

        let model = Model {
            route_service,
            route,
            storage,
            link,
            ws_location: "ws://localhost:4649/Spark".to_string(),
            wss: None,
            receivers: Vec::new(),
            radios: Vec::new(),
            default_receiver: None,
            version: None,
            spots: spot_db,
            audio: AudioProvider::new(),
            spectrum: SpectrumProvider::new(),
            show_receiver_list: false,
            import: entries,
            reader: ReaderService::new(),
            tasks: Vec::new(),
        };

        model.update_state_map_overlay();
        model
    }

    fn update_state_map_overlay(&self) {
        let (worked_states,lotw_states) =
            match &self.import {
                Some(import) => {
                    let worked_states : Vec<String> = import.iter().filter(|i| i.call.country() == Ok(Country::UnitedStates) && i.call.state().is_some()).map(|i| i.call.state().unwrap() ).collect();
                    let lotw_states : Vec<String> = import.iter().filter(|i| i.call.country() == Ok(Country::UnitedStates) && i.call.state().is_some() && i.lotw_qsl_rcvd).map(|i| i.call.state().unwrap() ).collect();
                    (worked_states,lotw_states)
                },
                None => {
                    (Vec::new(),Vec::new())
                }
            };

        let worked_states_json = serde_json::to_string(&worked_states).unwrap();
        let lotw_states_json = serde_json::to_string(&lotw_states).unwrap();

        let js = format!("workedStates = {};lotwConfirmed = {};updateStateOverlay();", worked_states_json, lotw_states_json);
        ConsoleService::debug(&format!("js: {}", js));
        js_sys::eval(&js).unwrap();
    }

    // CommandResponse: getReceiversResponse
    pub fn set_receivers(&mut self, receivers: Vec<Receiver>) {
        self.receivers = receivers;
        match self.default_receiver {
            None => {
                self.set_default_receiver(Some(self.receivers[0].id));
            },
            _ => ()
        }
    }

    pub fn default_receiver(&self) -> Option<Receiver> {
        match self.default_receiver {
            Some(receiver_id) => {
                if let Some(index) = self.receivers.iter().position(|i| i.id == receiver_id) {
                    Some(self.receivers[index].clone())
                } else {
                    None
                }
            },
            None => None
        }
    }

    // CommandResponse: getRadioResponse
    pub fn set_radios(&mut self, radios: Vec<Radio>) {
        self.radios = radios;
    }

    // CommandResponse: getVersionResponse
    pub fn set_version(&mut self, version: Version) {
        self.version = Some(version);
    }

    // CommandResponse: ReceiverResponse
    pub fn update_receiver(&mut self, receiver_id: u32, mode: Mode, frequency: f32, filter_low: f32, filter_high: f32) {
        if let Some(index) = self.receivers.iter().position(|i| i.id == receiver_id) {
            self.receivers[index].frequency = frequency;
            self.receivers[index].mode = mode;
            self.receivers[index].filter_low = filter_low;
            self.receivers[index].filter_high = filter_high;
            let receiver = self.receivers[index].clone();

            let js = &format!("initWaterfallNav(\"{}\", {}, {}, {});", receiver.mode.mode(), receiver.frequency, receiver.filter_high, receiver.filter_low);
            ConsoleService::log(&format!("js: {}", js));
            js_sys::eval(&js).unwrap();
        } else {
            ConsoleService::error(&format!("Attempted to update a receiver that does not exist: {}", receiver_id));
        }
    }

    pub fn change_receiver_mode(&mut self, receiver_id: u32, mode: Mode) {
        if let Some(index) = self.receivers.iter().position(|i| i.id == receiver_id) {
            self.receivers[index].mode = mode.clone();
            self.send_command(Command::SetMode { mode: mode.clone(), id: receiver_id });
        }
    }

    pub fn frequency_up(&mut self, receiver_id: u32, digit: i32) {
        if let Some(index) = self.receivers.iter().position(|i| i.id == receiver_id) {
            if digit == 0 { self.receivers[index].frequency += 100000000.0 }
            if digit == 1 { self.receivers[index].frequency += 10000000.0 }
            if digit == 2 { self.receivers[index].frequency += 1000000.0 }
            if digit == 3 { self.receivers[index].frequency += 100000.0 }
            if digit == 4 { self.receivers[index].frequency += 10000.0 }
            if digit == 5 { self.receivers[index].frequency += 1000.0 }
            if digit == 6 { self.receivers[index].frequency += 100.0 }
            if digit == 7 { self.receivers[index].frequency += 10.0 }
            if digit == 8 { self.receivers[index].frequency += 1.0 }

            self.send_command(Command::SetFrequency { frequency: (self.receivers[index].frequency as i32).to_string(), id: receiver_id });
        }
    }

    pub fn frequency_down(&mut self, receiver_id: u32, digit: i32) {
        if let Some(index) = self.receivers.iter().position(|i| i.id == receiver_id) {
            if digit == 0 { self.receivers[index].frequency -= 100000000.0 }
            if digit == 1 { self.receivers[index].frequency -= 10000000.0 }
            if digit == 2 { self.receivers[index].frequency -= 1000000.0 }
            if digit == 3 { self.receivers[index].frequency -= 100000.0 }
            if digit == 4 { self.receivers[index].frequency -= 10000.0 }
            if digit == 5 { self.receivers[index].frequency -= 1000.0 }
            if digit == 6 { self.receivers[index].frequency -= 100.0 }
            if digit == 7 { self.receivers[index].frequency -= 10.0 }
            if digit == 8 { self.receivers[index].frequency -= 1.0 }

            self.send_command(Command::SetFrequency { frequency: (self.receivers[index].frequency as i32).to_string(), id: receiver_id });
        }
    }

    // Two channels for the websocket connection
    // 1) Text: Json encoded messages for control/info (e.g. get/set frequency)
    // 2) Binary: Binary encoded audio data
    //
    // Both channels are bi-directional (e.g. transmit using binary encoded audio)
    // 
    pub fn connect(&mut self, ws: &str) {
        let ws = WebSocket::new(ws).unwrap();
        ws.set_binary_type(BinaryType::Arraybuffer);

		let cbnot = self.link.callback(|input| {
			match input {
				WebSocketStatus::Closed | WebSocketStatus::Error => {
					Msg::Disconnected
                },
                WebSocketStatus::Opened => {
                    Msg::Connected
                }
			}
        });
        
        let notify = cbnot.clone();
        let onopen_callback = Closure::wrap(Box::new(move |_| {
            ConsoleService::log("rig control: connection opened");
            notify.emit(WebSocketStatus::Opened);
        }) as Box<dyn FnMut(JsValue)>);
        ws.set_onopen(Some(onopen_callback.as_ref().unchecked_ref()));
        onopen_callback.forget();

        let notify = cbnot.clone();
        let onerror_callback = Closure::wrap(Box::new(move |_| {
            ConsoleService::error("rig control: connection error");
            notify.emit(WebSocketStatus::Error);
        }) as Box<dyn FnMut(JsValue)>);
        ws.set_onerror(Some(onerror_callback.as_ref().unchecked_ref()));
        onerror_callback.forget();

        let notify = cbnot.clone();
        let onclose_callback = Closure::wrap(Box::new(move |_| {
            ConsoleService::error("rig control: connection closed");
            notify.emit(WebSocketStatus::Closed);
        }) as Box<dyn FnMut(JsValue)>);
        ws.set_onclose(Some(onclose_callback.as_ref().unchecked_ref()));
        onclose_callback.forget();

        let cbout = self.link.callback(|data| {
            match data {
                WebsocketMsgType::BinaryMsg(binary) => {
                    Msg::ReceivedAudio(binary)
                },
                WebsocketMsgType::TextMsg(text) => {
                    let Json(data): Json<Result<CommandResponse, _>> = Json::from(Ok(text));
                    Msg::CommandResponse(data)
                }
            }
        });

        let onmessage_callback = Closure::wrap(Box::new(move |e: MessageEvent| {
            if let Ok(abuf) = e.data().dyn_into::<js_sys::ArrayBuffer>() {
                //let array = js_sys::Uint8Array::new(&abuf);
                cbout.emit(WebsocketMsgType::BinaryMsg(abuf));
            } else if let Ok(_blob) = e.data().dyn_into::<web_sys::Blob>() {
                ConsoleService::error("rig control: unexpected blob message from server");
            } else if let Ok(txt) = e.data().dyn_into::<js_sys::JsString>() {
                cbout.emit(WebsocketMsgType::TextMsg(txt.into()));
            } else {
                ConsoleService::error("rig control: unexpected message from server");
            }
        }) as Box<dyn FnMut(MessageEvent)>);
        ws.set_onmessage(Some(onmessage_callback.as_ref().unchecked_ref()));
        onmessage_callback.forget();
        self.wss = Some(ws)
    }

    pub fn disconnect(&mut self) {
        self.wss = None;
        self.receivers = Vec::new();
        self.radios = Vec::new();
        self.version = None;
        self.default_receiver = None;
        self.spots = SpotDB::new();
    }

    pub fn is_connected(&self) -> bool {
        match self.wss {
            Some(_) => true,
            None => false
        }
    }

    pub fn send_command(&mut self, cmd: Command) {
        let j = serde_json::to_string(&cmd).unwrap();
        if let Some(wss) = &self.wss {
            wss.send_with_str(&j).unwrap();
            ConsoleService::log(&format!("sent: {}", j));
        } else {
            ConsoleService::error(&format!("attempted to send: {}, but not connected", j));
        }
    }

    pub fn subscribe_to_audio(&mut self) {
        match self.audio.receiving_audio() {
            Some(previous_audio_channel) => {
                self.send_command(Command::SubscribeToAudio{ rx_id: previous_audio_channel, enable: false });
            },
            None => ()
        }
        let rx_id =
            match self.default_receiver() {
                Some(receiver) => {
                    self.send_command(Command::SubscribeToAudio{ rx_id: receiver.id, enable: true });
                    ConsoleService::log(&format!("subscribed to audio channel: {}", receiver.id));
                    Some(receiver.id)
                },
                None => None,
            };
        self.audio.set_subscribed(rx_id);
    }

    pub fn unsubscribe_to_audio(&mut self) {
        match self.audio.receiving_audio() {
            Some(previous_audio_channel) => {
                self.send_command(Command::SubscribeToAudio{ rx_id: previous_audio_channel, enable: false });
                ConsoleService::log("unsubscribed to audio");
            },
            None => ()
        }
        self.audio.set_subscribed(None);
    }

    pub fn set_default_receiver(&mut self, receiver: Option<u32>) {
        if self.default_receiver == receiver { /* do nothing */ }
        else {
            // unsubscribe to old spectrum data
            match self.spectrum.receiving_spectrum() {
                Some(previous_subscription) => {
                    self.send_command(Command::SubscribeToSpectrum{ rx_id: previous_subscription, enable: false });
                },
                None => ()
            }

            // subscribe to new spectrum data
            match receiver {
                Some(receiver_id) => {
                    if let Some(index) = self.receivers.iter().position(|i| i.id == receiver_id) {
                        let receiver = self.receivers[index].clone();
                        self.send_command(Command::SubscribeToSpectrum{ rx_id: receiver_id, enable: true });
                        self.spectrum.set_subscribed(Some(receiver_id));

                        let js = format!("initWaterfallNav(\"{}\", {}, {}, {});", receiver.mode.mode(), receiver.frequency, receiver.filter_high, receiver.filter_low);
                        ConsoleService::log(&format!("js: {}", js));
                        js_sys::eval(&js).unwrap();

                        // update default receiver
                        self.default_receiver = Some(receiver_id);

                        // switch audio subscriptions if already subscribed
                        match self.audio.receiving_audio() {
                            Some(_) => {
                                self.subscribe_to_audio();
                            },
                            None => ()
                        }
                    } else {
                        ConsoleService::error(&format!("Attempted to set default receiver with invalid receiver id: {}", receiver_id));
                    }
                },
                None => {
                    self.default_receiver = None;
                    self.unsubscribe_to_audio();
                    js_sys::eval("initWaterfallNav(null, null, null, null);").unwrap();
                }
            }
        }
    }

    pub fn toggle_receiver_list(&mut self) {
        self.show_receiver_list = !self.show_receiver_list;
    }

    pub fn show_receiver_list(&mut self) {
        self.show_receiver_list = true
    }

    pub fn hide_receiver_list(&mut self) {
        self.show_receiver_list = false
    }

    pub fn read_file(&mut self, file: File) {
        let task = {
            let callback = self.link.callback(|data| Msg::Loaded(data));
            self.reader.read_file(file, callback).unwrap()
        };
        self.tasks.push(task);
    }

    pub fn load_adif_data(&mut self, data: FileData) {
        match ham_rs::adif::adif_parse("import", &mut data.content.as_slice()) {
            Ok(adif) => {
                let mut records = Vec::new();
                for record in adif.adif_records.as_slice() {
                    match LogEntry::from_adif_record(&record) {
                        Ok(entry) => {
                            records.push(entry);
                        },
                        Err(e) => {
                            ConsoleService::error(&format!("failed to import record [{:?}]: {:?}", e, record));
                        }
                    }
                }
                self.import = Some(records);
                self.storage.store(LOGBOOK_KEY, Json(&self.import));
                self.update_state_map_overlay();
            },
            Err(e) => {
                ConsoleService::error(&format!("unable to load adif: {}", e));
            }
        }
    }

    pub fn clear_adif_data(&mut self) {
        self.import = None;
        self.storage.store(LOGBOOK_KEY, Json(&self.import));
        self.update_state_map_overlay();
    }

    pub fn get_radio_power_state(&self, radio_id: u32) -> Option<bool> {
        if let Some(index) = self.radios.iter().position(|i| i.id == radio_id) {
            Some(self.radios[index].running)
        } else {
            None
        }
    }

    pub fn toggle_receivers_button(&self) -> Html {
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

    pub fn receiver_list_control(&self) -> Html {
        html! {
            <div class="receivers">
            {
                for self.receivers.iter().map(|r| {
                    self.receiver(&r)
                })
            }
            </div>
        }
    }

    pub fn spots_view(&self) -> Html {
        let table_class =
            match self.default_receiver() {
                Some(receiver) if receiver.has_spots() && self.spots.current_receiver_spot_filter_enabled() => {
                    "table is-narrow is-fullwidth filter-currentrx"
                },
                _ => "table is-narrow is-fullwidth",
            };

        html! {
            <>
                <div style="text-align:right;margin-top:10px">
                    <button class="button" onclick=self.link.callback(move |_| Msg::ClearSpots)>
                        <span class="icon is-small">
                            <i class="far fa-trash-alt"></i>
                        </span>
                    </button>
                </div>
                <div class="s">
                    <table class=table_class>
                        <tr>
                            <th>{ "UTC" }</th>
                            <th>{ "dB" }</th>
                            <th>{ "DT" }</th>
                            <th class="freqc">{ "Freq" }</th>
                            <th class="modec">{ "Mode" }</th>
                            <th>{ "Dist" }</th>
                            <th>{ "Message" }</th>
                            <th></th>
                            <th></th>
                            <th></th>
                            {
                                match self.spots.has_lotw_users() {
                                    true => html! { <th>{ "LoTW" }</th> },
                                    false => html! {}
                                }
                            }
                        </tr>
                        { for self.spots.spots().iter().rev().map(|s| {
                            self.spot(&s)
                          })
                        }
                    </table>
                </div>
            </>
        }
    }

    pub fn spot_filters_sidebar(&self) -> Html {
        let default_receiver_has_spots =
            match self.default_receiver() {
                Some(receiver) if receiver.has_spots() => true,
                _ => false,
            };

        html! {
            <div class="receiver-control spot-filters">
                <table class="table is-fullwidth">
                    <thead>
                        <tr>
                            <th colspan="2">{ "Spot Filters" }</th>
                        </tr>
                    </thead>
                    <tbody>
                        <tr>
                            <td>{ "CQ Only" }</td>
                            <td style="text-align:right">
                                <label class="switch">
                                    <input id="switchColorDefault" type="checkbox" name="switchColorDefault" checked=self.spots.cq_only_spot_filter_enabled() onclick=self.link.callback(move |_| Msg::ToggleCQSpotFilter ) />
                                    <span class="slider"></span>
                                </label>
                            </td>
                        </tr>
                        { if default_receiver_has_spots {
                            html! {
                                <tr>
                                    <td>{ "Current Rx" }</td>
                                    <td style="text-align:right">
                                        <label class="switch">
                                            <input id="switchColorDefault" type="checkbox" name="switchColorDefault" checked=self.spots.current_receiver_spot_filter_enabled() onclick=self.link.callback(move |_| Msg::ToggleCurrentReceiverSpotFilter ) />
                                            <span class="slider"></span>
                                        </label>
                                    </td>
                                </tr>
                            } } else {
                                html! {}
                            }
                        }
                        { if self.spots.has_lotw_users() {
                            html! {
                                <tr>
                                    <td>{ "LoTW" }</td>
                                    <td style="text-align:right">
                                        <label class="switch">
                                            <input id="switchColorDefault" type="checkbox" name="switchColorDefault" checked=self.spots.lotw_spot_filter_enabled() onclick=self.link.callback(move |_| Msg::ToggleLoTWSpotFilter ) />
                                            <span class="slider"></span>
                                        </label>
                                    </td>
                                </tr>
                            } } else {
                                html! {}
                            }
                        }
                        { if let Some(_) = self.import { 
                              html! {
                                <>
                                <tr>
                                    <td>{ "New State" }</td>
                                    <td style="text-align:right">
                                        <label class="switch">
                                            <input id="switchColorDefault" type="checkbox" name="switchColorDefault" checked=self.spots.state_spot_filter_enabled() onclick=self.link.callback(move |_| Msg::ToggleStateSpotFilter ) />
                                            <span class="slider"></span>
                                        </label>
                                    </td>
                                </tr>
                                <tr>
                                    <td>{ "New Country" }</td>
                                    <td style="text-align:right">
                                        <label class="switch">
                                            <input id="switchColorDefault" type="checkbox" name="switchColorDefault" checked=self.spots.country_spot_filter_enabled() onclick=self.link.callback(move |_| Msg::ToggleCountrySpotFilter ) />
                                            <span class="slider"></span>
                                        </label>
                                    </td>
                                </tr>
                                </>
                              }
                          } else {
                              html! {}
                        }}
                    </tbody>
                    <thead>
                        <tr>
                            <th colspan="2">{ "Log File" }</th>
                        </tr>
                    </thead>
                    <tbody>
                        <tr>
                            <td colspan="2">{ self.import_adif_form() }</td>
                        </tr>
                    </tbody>
                </table>
            </div>
        }
    }
    pub fn version_html(&self) -> Html {
        match &self.version {
            Some(version) => {
                let host_url =
                    match version.host.as_str() {
                        "SparkSDR" => "http://www.ihopper.org/radio/",
                        _ => "",
                    };
                html! { 
                    <p class="version"><a href=host_url>{ version.host.to_string() }</a>{ format!(" {} [Protocol Version: {}]", version.host_version, version.protocol_version) }</p> 
                }
            },
            None => html! {},
        }
    }

    fn import_adif_form(&self) -> Html {
        html! {
                <div class="import">
                    {
                        match &self.import {
                            None => html! {
                                <>
                    <p>{"Compare spots to log file to find new countries and states."}</p>
                    <input class="file-import" type="file" multiple=true onchange=self.link.callback(move |value| {
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
                    <p><i>{ "(adif only)" }</i></p>
                                </>
                            },
                            Some(import) => html! {
                                <>
                                    <p>{ format!("Loaded {} contacts", import.len()) }</p>
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

    fn spot(&self, spot: &Spot) -> Html {
        let (country_icon, state_class) =
            match spot.call.country() {
                Ok(country) => {
                    let (new_country, new_state) =
                        match &self.import {
                            Some(import) => {
                                let new_country =
                                    if let Some(_index) = import.iter().position(|i| i.call.country() == Ok(country.clone())) {
                                        ""
                                    } else {
                                        "has-text-success"
                                    };
                                let new_state =
                                    match spot.call.state() {
                                        Some(state) => {
                                            if let Some(_index) = import.iter().position(|i| i.call.state() == Some(state.to_string())) {
                                                ""
                                            } else {
                                                "has-text-success"
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

        let (lotw_enabled, uses_lotw) =
            match spot.call.lotw() {
                LoTWStatus::LastUpload(_) | LoTWStatus::Registered => (true, html! { <span class="has-text-success">{ "Yes" }</span> }),
                LoTWStatus::Unregistered => (true, html! { { "No" } }),
                LoTWStatus::Unknown => (false, html! {})
            };

        let spot_receiver_id =
            if let Some(index) = self.receivers.iter().position(|i| i.frequency == spot.tuned_frequency && i.mode == spot.mode ) {
                Some(self.receivers[index].id)
            } else {
                None
            };

        html! {
            <tr>
                <td>{ spot.time.format("%H%M%S") }</td>
                <td>{ spot.snr }</td>
                <td>{ spot.dt }</td>
                <td class="freqc"><span>{ format!("{} (+", spot.tuned_frequency) }</span>{ format!("{}", (spot.frequency - spot.tuned_frequency)) }<span>{ ")" }</span></td>
                <th class="modec">{ spot.mode.mode() }</th>
                <td>{ match spot.distance {
                         Some(dist) => format!("{}", dist),
                         None => format!(""),
                      }
                    }</td>
                {
                    if let Some(msg) = &spot.msg {
                        match (msg.contains("CQ"), spot_receiver_id) {
                            (true, Some(receiver_id)) => html! { <th><a onclick=self.link.callback(move |_| Msg::SetDefaultReceiver(receiver_id) )>{ msg.to_string() }</a></th> },
                            (true, None) => html! { <th>{ msg.to_string() }</th> },
                            (false, _) => html! { <td>{ msg.to_string() }</td> }
                        }
                    } else {
                        html! { <td>{ "--" }</td> }
                    }
                }
                <td>{ country_icon }</td>
                <td class=state_class>{ match spot.call.state() {
                          Some(state) => format!("{}", state),
                          None => format!("")
                      } }</td>
                <td>{ match spot.call.op() {
                          Some(op) => format!("{}", op),
                          None => format!("")
                      } }</td>
                {
                    match lotw_enabled {
                        true => html! { <td>{ uses_lotw }</td> },
                        false => html! {}
                    }
                }
            </tr>
        }
    }

    pub fn receiver(&self, receiver: &Receiver) -> Html {
        let frequency_string = format!("{:0>9}", receiver.frequency.to_string());
        let tmp = self.decimal_mark(frequency_string);
        let mut inactive = true;
        let receiver_id = receiver.id;
        let (class_name, is_default) = 
            if Some(receiver.id) == self.default_receiver {
                if !self.show_receiver_list {
                    ("receiver-control selected main-view has-background-light", true)
                } else {
                    ("receiver-control selected has-background-light", true)
                }
            } else {
                ("receiver-control", false)
            };
        let mute_unmute_main_class =
            match self.audio.receiving_audio() {
                Some(_) => "icon is-small",
                None => "icon is-small has-text-danger",
            };

        if self.show_receiver_list || is_default {
        html! {
            <div class=class_name onclick=self.link.callback(move |_| Msg::SetDefaultReceiver(receiver_id))>
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
                    {
                        if self.show_receiver_list {
                            html! {
                                <button style="float:right" class="button is-text" onclick=self.link.callback(move |_| Msg::RemoveReceiver(receiver_id))>
                                    <span class="icon is-small">
                                    <i class="far fa-trash-alt"></i>
                                    </span>
                                </button>
                            }
                        } else {
                            html! {}
                        }
                    }
                    { if is_default {
                            html! {
                                <button style="float:right" class="button is-text" onclick=self.link.callback(move |_| Msg::EnableAudio )>
                                    <span class=mute_unmute_main_class>
                                        <i class="fas fa-volume-up"></i>
                                    </span>
                                </button>
                            }
                        } else {
                            html! { }
                        }
                    }
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
            </div>
        }
        } else {
            html! {}
        }
    }

    fn decimal_mark(&self, s: String) -> String {
        let bytes: Vec<_> = s.bytes().rev().collect();
        let chunks: Vec<_> = bytes.chunks(3).map(|chunk| str::from_utf8(chunk).unwrap()).collect();
        let result: Vec<_> = chunks.join(",").bytes().rev().collect();
        String::from_utf8(result).unwrap()
    }

    fn radio_navbar_controls(&self, radio: &Radio) -> Html {
        let radio_id = radio.id;
        let power_class =
            match radio.running {
                true => "icon is-small has-text-success",
                false => "icon is-small",
            };
        let short_name = &radio.name[14..];
        html! {
            <>
                <a class="navbar-item" disabled=true>
                    { short_name }
                </a>
                <div class="navbar-item">
                    <div class="field has-addons">
                        <p class="control">
                            <button class="button" title="Power" onclick=self.link.callback(move |_| Msg::TogglePower(radio_id))>
                                <span class=power_class>
                                <i class="fas fa-power-off fa-lg"></i>
                                </span>
                            </button>
                        </p>
                        <p class="control">
                            <button class="button" onclick=self.link.callback(move |_| Msg::AddReceiver(radio_id) ) title="Add Receiver">
                                <span class="icon is-small">
                                <i class="fas fa-plus fa-lg"></i>
                                </span>
                            </button>
                        </p>
                    </div>
                </div>
            </>
        }
    }

    pub fn navbar_view(&self) -> Html {
        let cls = if self.show_receiver_list == true {
            "fa-chevron-up"
        } else {
            "fa-chevron-down"
        };
        let (spot_class, map_class) =
            match AppRoute::switch(self.route.clone()) {
                Some(AppRoute::Index) => ("navbar-item is-active", "navbar-item"),
                Some(AppRoute::Map) => ("navbar-item", "navbar-item is-active"),
                None => ("navbar-item is-active","navbar-item"),
            };
            
        html! {
            <nav class="navbar is-light" role="navigation" aria-label="main navigation">
                <div class="navbar-brand">
                    { for self.radios.iter().map(|r| {
                        self.radio_navbar_controls(&r)
                      })
                    }
                    <a class="navbar-item" onclick=self.link.callback(move |_| Msg::ToggleReceiverList)>
                        <span>{ format!("{} Receivers ", self.receivers.len()) }</span>
                        <span class="icon is-small">
                            <i class=("fas", cls)></i>
                        </span>
                    </a>

                    <div class="navbar-burger burger" data-target="radioNavigation">
                        <span></span>
                        <span></span>
                        <span></span>
                    </div>
                </div>
                <div id="radioNavigation" class="navbar-menu">
                    <div class="navbar-start">


                        <a class=spot_class onclick=self.link.callback(|_| Msg::ChangeRoute(AppRoute::Index))>
                            { "Spots" }
                        </a>

                        <a class=map_class onclick=self.link.callback(|_| Msg::ChangeRoute(AppRoute::Map))>
                            { "Map" }
                        </a>

                    </div>
                </div>
            </nav>
        }
    }

    pub fn footer_view(&self) -> Html {
        html! {
            <div class="copy">
                <div class=if self.is_connected() { "" } else { "container" }>
                    { self.version_html() }
                    <p><a href="https://github.com/nricciar/sparksdr-websocket-demo" target="_blank">{ "sparksdr-websocket-demo @ github" }</a></p>
                </div>
            </div>
        }
    }

    pub fn disconnected_view(&self) -> Html {
        html! {
            <>
                <div class="container">
                    <h1 class="title">{ "Disconnected" }</h1>
                    <p>{ "Make sure SparkSDR has Web Sockets enabled, and hostname is correct "}</p>
                    <div class="field is-grouped ws-connection">
                    <input class="input"
                        value=&self.ws_location
                        oninput=self.link.callback(|e: InputData| Msg::UpdateWebsocketAddress(e.value))
                        onkeypress=self.link.callback(|e: KeyboardEvent| {
                            if e.key() == "Enter" { Msg::Connect } else { Msg::None }
                        }) />
                    <button class="button is-link" onclick=self.link.callback(move |_| Msg::Connect )>
                        { "Connect" }
                    </button>
                    </div>
                </div>
                { self.footer_view() }
            </>
        }
    }
}
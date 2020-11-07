#![recursion_limit = "2048"]
use anyhow::Error;
use wasm_bindgen::prelude::*;
use yew::prelude::*;
use yew::services::{ConsoleService,Task};
use yew::{html, Component, ComponentLink, Html, ShouldRender};
use yew_router::{route::Route, service::RouteService};
use yew_router::{Switch};
use yew::format::{Json,Nothing};
use yew::services::interval::{IntervalService};
use yew::services::fetch::{FetchService, FetchTask, Request, Response};
use yew::services::reader::{File, FileData, ReaderService, ReaderTask};
use yew::services::websocket::{WebSocketStatus};//, WebSocketTask};
use web_sys::{WebSocket,BinaryType,MessageEvent};
use wasm_bindgen::JsCast;
use uuid::Uuid;
use std::str;
use std::time::Duration;
use adif;

use ham_rs::Call;
use ham_rs::countries::{CountryInfo,Country};
use ham_rs::rig::{Receiver,Radio,Version,Command,CommandResponse,RECEIVER_MODES,Mode,Spot};
use ham_rs::log::LogEntry;

// Currently this is unused as there is only one route: /
#[derive(Clone,Switch, Debug)]
pub enum AppRoute {
    #[to = "/"]
    Index,
}

// Used with the local callsign cache for our requests
// for callsign info.
pub enum CallsignInfo {
    Requested((Call, FetchTask)),
    Found(Call),
    NotFound(Call)
}

impl CallsignInfo {
    fn call(&self) -> Call {
        match self {
            CallsignInfo::Requested((c, _)) => c.clone(),
            CallsignInfo::Found(c) => c.clone(),
            CallsignInfo::NotFound(c) => c.clone(),
        }
    }
}

pub struct Model {
    // Currently unused
    route_service: RouteService<()>,
    route: Route<()>,
    // Callbacks
    link: ComponentLink<Self>,
    // Console logging
    console: ConsoleService,
    // SparkSDR connection
    wss: WebSocket,
    // List of receivers from getReceivers command
    receivers: Vec<Receiver>,
    // List of radios from the getRadios command
    radios: Vec<Radio>,
    // Currently selected receiver
    default_receiver: Option<Uuid>,
    // Version response from the getVersion command
    version: Option<Version>,
    // TODO: temporary to keep local ui updated
    poll: Option<Box<dyn Task>>,
    // Spots from enabling SubscribeToSpots
    spots: Vec<Spot>,
    // Show/Hide receiver list
    show_receiver_list: bool,
    // Imported log file (ADIF format) for spot cross checking
    import: Option<Vec<LogEntry>>,
    // Services for file importing (log file)
    reader: ReaderService,
    tasks: Vec<ReaderTask>,
    // Local callsign cache
    callsigns: Vec<CallsignInfo>,
}

// Currently only TextMsg is implemented and this is the
// commands to and responses from SparkSDR.
// BinaryMsg would be for future support for audio in/out
enum WebsocketMsgType {
    BinaryMsg(js_sys::ArrayBuffer),
    TextMsg(String)
}

type Chunks = bool;

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

impl Component for Model {
    type Message = Msg;
    type Properties = ();

    fn update(&mut self, msg: Self::Message) -> ShouldRender {
        match msg {
            Msg::ReceivedText(Ok(msg)) => {
                match msg {
                    // getReceiversResponse: update our receiver list
                    CommandResponse::Receivers { Receivers: receivers } => {
                        self.receivers = receivers;
                    },
                    // getRadioResponse: update our radio list
                    CommandResponse::Radios { Radios: radios } => {
                        self.radios = radios;
                    },
                    // getVersionResponse: update our version info
                    CommandResponse::Version(version) => {
                        self.version = Some(version);
                    },
                    // spotResponse: new incoming spots
                    CommandResponse::Spots { Spots: spots } => {
                        for mut spot in spots {
                            // check callsign cache to see if we have info already
                            if let Some(index) = self.callsigns.iter().position(|c| c.call().call() == spot.call.call() ) {
                                match &self.callsigns[index] {
                                    CallsignInfo::Found(call) => {
                                        // update spot call with additional callsign info from cache
                                        let call = call.clone();
                                        spot.call = call;
                                    },
                                    _ => ()
                                }
                            } else {
                                let call = spot.call.clone();
                                let callback = self.link.callback(
                                    move |response: Response<Json<Result<Call, Error>>>| {
                                        let (meta, Json(data)) = response.into_parts();
                                        if meta.status.is_success() {
                                            Msg::CallsignInfoReady(data)
                                        } else {
                                            Msg::None // FIXME: Handle this error accordingly.
                                        }
                                    },
                                );

                                match call.prefix() {
                                    // If callsign is United States make a request for additional callsign
                                    // info from server.  Response will be handled by the Msg::CallsignInfoReady
                                    // message handler
                                    Some(prefix) if call.country() == Ok(Country::UnitedStates) => {
                                        let mut fs = FetchService::new();
                                        let request = Request::get(format!("/out/{}/{}.json", prefix, spot.call.call())).body(Nothing).unwrap();
                                        let ft = fs.fetch(request, callback).unwrap();

                                        let info = CallsignInfo::Requested((call, ft));
                                        self.callsigns.push(info);
                                    },
                                    _ => ()
                                }
                            }

                            self.spots.push(spot);
                        }

                        // keep only the 100 most recent spots
                        if self.spots.len() > 100 {
                            let drain = self.spots.len() - 100;
                            self.spots.drain(0..drain);
                        }
                    }
                }
            },
            Msg::CallsignInfoReady(Ok(call)) => {
                // Find every spot record for callsign
                let indexes : Vec<usize> = self.spots.iter().enumerate().filter(|&(_, s)| s.call.call() == call.call() ).map(|(i, _)| i).collect();
                for index in indexes {
                    // update spot record with our updated callsign info
                    self.spots[index].call = call.clone();
                }

                // Mark callsign as found in local callsign cache for
                // future lookups
                if let Some(index) = self.callsigns.iter().position(|c| c.call().call() == call.call()) {
                    self.callsigns[index] = CallsignInfo::Found(call)
                } else {
                    self.callsigns.push(CallsignInfo::Found(call))
                }
            },
            Msg::CallsignInfoReady(Err(err)) => {
                let msg = format!("callsign info error: {}", err);
                self.console.log(&msg);
            },
            Msg::SetDefaultReceiver(receiver_id) => {
                self.default_receiver = Some(receiver_id);
            },
            Msg::AddReceiver(radio_id) => {
                self.send_command(Command::AddReceiver { ID: radio_id });
            },
            Msg::Tick => {
                // Not all SparkSDR commands currently provide a response.
                // To fake it we poll getReceivers every 10 seconds to keep ui
                // in sync with backend.
                self.send_command(Command::GetReceivers);
            },
            Msg::ToggleReceiverList => {
                self.show_receiver_list = !self.show_receiver_list;
            },
            Msg::ModeChanged(receiver_id, mode) => {
                if let Some(index) = self.receivers.iter().position(|i| i.id == receiver_id) {
                    self.receivers[index].mode = mode.clone();
                    self.send_command(Command::SetMode { Mode: mode.clone(), ID: receiver_id });
                }
            },
            Msg::FrequencyDown(receiver_id, digit) => {
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

                    self.send_command(Command::SetFrequency { Frequency: (self.receivers[index].frequency as i32).to_string(), ID: receiver_id });
                }
            },
            Msg::FrequencyUp(receiver_id, digit) => {
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

                    self.send_command(Command::SetFrequency { Frequency: (self.receivers[index].frequency as i32).to_string(), ID: receiver_id });
                }
            }
            Msg::ReceivedText(Err(err)) => {
                let err = format!("error: {}", err);
                self.console.log(&err);
            },
            Msg::Disconnected => {
                self.console.log("Disconnected");
            },
            Msg::Connected => {
                // When we first connect to SparkSDR gather some basic information
                self.send_command(Command::GetReceivers);
                self.send_command(Command::GetRadios);
                self.send_command(Command::GetVersion);
                // Also subscribe to spots
                self.send_command(Command::SubscribeToSpots{ Enable: true });
            },
            Msg::ReceivedAudio(_data) => {
                // TODO: do stuff
            },
            Msg::RouteChanged(route) => {
                self.route = route;
            },
            Msg::ChangeRoute(route) => {
                // This might be derived in the future
                self.route = route.into();
                self.route_service.set_route(&self.route.route, ());
            },
            Msg::CancelImport => {
                self.import = None;
            },
            Msg::ConfirmImport => {
            },
            Msg::Loaded(data) => {
                match adif::adif_parse("import", &mut data.content.as_slice()) {
                    Ok(adif) => {
                        let mut records = Vec::new();
                        for record in adif.adif_records.as_slice() {
                            match LogEntry::from_adif_record(&record) {
                                Ok(entry) => {
                                    records.push(entry);
                                },
                                Err(e) => {
                                    self.console.log(&format!("failed to import record [{:?}]: {:?}", e, record));
                                }
                            }
                        }
                        self.import = Some(records);
                    },
                    Err(e) => {
                        self.console.log(&format!("unable to load adif: {}", e));
                    }
                }
            },
            Msg::Files(files, _) => {
                for file in files.into_iter() {
                    let task = {
                        let callback = self.link.callback(|data| Msg::Loaded(data));
                        self.reader.read_file(file, callback).unwrap()
                    };
                    self.tasks.push(task);
                }
            },
            Msg::None => {}
        }
        true
    }

    fn create(_props: Self::Properties, link: ComponentLink<Self>) -> Self {
        let mut route_service: RouteService<()> = RouteService::new();
        let route = route_service.get_route();
        let callback = link.callback(Msg::RouteChanged);
        route_service.register_callback(callback);

        // TODO: temporary fix to keep ui in sync with backend
        let mut is = IntervalService::new();
        let handle = is.spawn(
            Duration::from_secs(10),
            link.callback(|_| Msg::Tick),
        );

        // Websocket for rig control
        // Two channels for the websocket connection
        // 1) Text: Json encoded messages for control/info (e.g. get/set frequency)
        // 2) Binary: Binary encoded audio data
        //
        // Both channels are bi-directional (e.g. transmit using binary encoded audio)
        // 
        let ws = WebSocket::new("ws://localhost:4649/Spark").unwrap();
        ws.set_binary_type(BinaryType::Arraybuffer);

		let cbnot = link.callback(|input| {
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
            ConsoleService::new().log("rig control: connection opened");
            notify.emit(WebSocketStatus::Opened);
        }) as Box<dyn FnMut(JsValue)>);
        ws.set_onopen(Some(onopen_callback.as_ref().unchecked_ref()));
        onopen_callback.forget();

        let notify = cbnot.clone();
        let onerror_callback = Closure::wrap(Box::new(move |_| {
            ConsoleService::new().log("rig control: connection error");
            notify.emit(WebSocketStatus::Error);
        }) as Box<dyn FnMut(JsValue)>);
        ws.set_onerror(Some(onerror_callback.as_ref().unchecked_ref()));
        onerror_callback.forget();

        let notify = cbnot.clone();
        let onclose_callback = Closure::wrap(Box::new(move |_| {
            ConsoleService::new().log("rig control: connection closed");
            notify.emit(WebSocketStatus::Closed);
        }) as Box<dyn FnMut(JsValue)>);
        ws.set_onclose(Some(onclose_callback.as_ref().unchecked_ref()));
        onclose_callback.forget();

        let cbout = link.callback(|data| {
            match data {
                WebsocketMsgType::BinaryMsg(binary) => {
                    Msg::ReceivedAudio(binary)
                },
                WebsocketMsgType::TextMsg(text) => {
                    let Json(data): Json<Result<CommandResponse, _>> = Json::from(Ok(text));
                    Msg::ReceivedText(data)
                }
            }
        });
        let onmessage_callback = Closure::wrap(Box::new(move |e: MessageEvent| {
            if let Ok(abuf) = e.data().dyn_into::<js_sys::ArrayBuffer>() {
                //let array = js_sys::Uint8Array::new(&abuf);
                cbout.emit(WebsocketMsgType::BinaryMsg(abuf));
            } else if let Ok(_blob) = e.data().dyn_into::<web_sys::Blob>() {
                ConsoleService::new().log("rig control: unexpected blob message from server");
            } else if let Ok(txt) = e.data().dyn_into::<js_sys::JsString>() {
                cbout.emit(WebsocketMsgType::TextMsg(txt.into()));
            } else {
                ConsoleService::new().log("rig control: unexpected message from server");
            }
        }) as Box<dyn FnMut(MessageEvent)>);
        ws.set_onmessage(Some(onmessage_callback.as_ref().unchecked_ref()));
        onmessage_callback.forget();

        // audio channel
        let audio_ctx = web_sys::AudioContext::new().unwrap();
        let gain = audio_ctx.create_gain().unwrap();
        gain.gain().set_value(1.0);
        gain.connect_with_audio_node(&audio_ctx.destination()).unwrap();
        let source = audio_ctx.create_buffer_source().unwrap();
        source.start().unwrap();
        source.set_loop(true);

        Model {
            route_service,
            route,
            link,
            console: ConsoleService::new(),
            wss: ws,
            receivers: vec![],
            radios: vec![],
            default_receiver: None,
            version: None,
            poll: Some(Box::new(handle)),
            spots: vec![],
            show_receiver_list: false,
            import: None,
            reader: ReaderService::new(),
            tasks: Vec::new(),
            callsigns: vec![]
        }
    }

    fn change(&mut self, _: Self::Properties) -> ShouldRender {
        true
    }

    fn view(&self) -> Html {
        html! {
            <>
                { for self.radios.iter().map(|r| {
                    self.radio(&r)
                  })
                }

                <div class="control-bar">
                    { self.toggle_receivers() }
                </div>

                <div style="clear:both"></div>

                {
                    match self.show_receiver_list {
                        true => {
                            html! {
                                {
                                    for self.receivers.iter().map(|r| {
                                        self.receiver(&r)
                                    })
                                }
                            }
                        },
                        false => html! {}
                    }
                }

                { self.spots_view() }

                {
                    match &self.version {
                        Some(version) => html! { <p class="version">{ format!("{} {} [Protocol Version: {}]", version.host, version.host_version, version.protocol_version) }</p> },
                        None => html! {},
                    }
                }
            </>
        }
    }
}

impl Model {
    fn toggle_receivers(&self) -> Html {
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
    fn spots_view(&self) -> Html {
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

    fn send_command(&mut self, cmd: Command) {
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

    fn radio(&self, radio: &Radio) -> Html {
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

    fn receiver(&self, receiver: &Receiver) -> Html {
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

#[wasm_bindgen(start)]
pub fn run_app() {
    //App::<Model>::new().mount_to_body();
    yew::start_app::<Model>();
}
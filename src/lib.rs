#![recursion_limit = "2048"]
use anyhow::Error;
use wasm_bindgen::prelude::*;
use yew::prelude::*;
use yew::services::{ConsoleService,Task};
use yew::{html, Component, ComponentLink, Html, ShouldRender};
use yew_router::{route::Route, service::RouteService};
use yew_router::{Switch};
use yew::format::{Json};
use yew::services::interval::{IntervalService, IntervalTask};
//use yew::services::fetch::{FetchService, FetchTask, Request, Response};
use yew::services::websocket::{WebSocketStatus};//, WebSocketTask};
use web_sys::{WebSocket,BinaryType,MessageEvent};
use wasm_bindgen::JsCast;
use uuid::Uuid;
use std::str;
use std::time::Duration;
use chrono::prelude::*;

use ham_rs::Call;
use ham_rs::countries::CountryInfo;
use ham_rs::rig::{Receiver,Radio,Version,Command,CommandResponse,RECEIVER_MODES,Mode,Spot};

#[derive(Clone,Switch, Debug)]
pub enum AppRoute {
    #[to = "/"]
    Index,
}

pub struct Model {
    route_service: RouteService<()>,
    route: Route<()>,
    link: ComponentLink<Self>,
    console: ConsoleService,
    //ft: Option<FetchTask>,
    //ws: Option<WebSocketTask>,
    wss: WebSocket,
    receivers: Vec<Receiver>,
    radios: Vec<Radio>,
    default_receiver: Option<Uuid>,
    version: Option<Version>,
    poll: Option<Box<dyn Task>>,
    spots: Vec<Spot>
}

enum WebsocketMsgType {
    BinaryMsg(js_sys::ArrayBuffer),
    TextMsg(String)
}

pub enum Msg {
    RouteChanged(Route<()>),
    ChangeRoute(AppRoute),
    Disconnected,
    Connected,
    ReceivedText(Result<CommandResponse, Error>),
    FrequencyUp(Uuid, i32), // digit 0 - 8 
    FrequencyDown(Uuid, i32), // digit 0 - 8
    ModeChanged(Uuid, Mode),
    SetDefaultReceiver(Uuid),
    AddReceiver(Uuid),
    ReceivedAudio(js_sys::ArrayBuffer),
    None,
    Tick
}

impl Component for Model {
    type Message = Msg;
    type Properties = ();

    fn update(&mut self, msg: Self::Message) -> ShouldRender {
        match msg {
            Msg::ReceivedText(Ok(msg)) => {
                match msg {
                    CommandResponse::Receivers { Receivers: receivers } => {
                        self.receivers = receivers;
                    },
                    CommandResponse::Radios { Radios: radios } => {
                        self.radios = radios;
                    },
                    CommandResponse::Version(version) => {
                        self.version = Some(version);
                    },
                    CommandResponse::Spots { Spots: mut spots } => {
                        self.spots.append(&mut spots);
                        if self.spots.len() > 100 {
                            let drain = self.spots.len() - 100;
                            self.spots.drain(0..drain);
                        }
                    }
                }
            },
            Msg::SetDefaultReceiver(receiver_id) => {
                self.default_receiver = Some(receiver_id);
            },
            Msg::AddReceiver(radio_id) => {
                self.send_command(Command::AddReceiver { ID: radio_id });
            },
            Msg::Tick => {
                self.send_command(Command::GetReceivers);
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
                self.send_command(Command::GetReceivers);
                self.send_command(Command::GetRadios);
                self.send_command(Command::GetVersion);
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
            Msg::None => {}
        }
        true
    }

    fn create(_props: Self::Properties, link: ComponentLink<Self>) -> Self {
        let mut route_service: RouteService<()> = RouteService::new();
        let route = route_service.get_route();
        let callback = link.callback(Msg::RouteChanged);
        route_service.register_callback(callback);

        let mut is = IntervalService::new();
        let handle = is.spawn(
            Duration::from_secs(10),
            link.callback(|_| Msg::Tick),
        );

        //let mut fs = FetchService::new();

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
            //ft: None,
            wss: ws,
            //ws: None,
            receivers: vec![],
            radios: vec![],
            default_receiver: None,
            version: None,
            poll: Some(Box::new(handle)),
            spots: vec![]
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

                <div style="clear:both"></div>

                { for self.receivers.iter().map(|r| {
                    self.receiver(&r)
                  })
                }

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
                        </tr>
                        { for self.spots.iter().rev().map(|s| {
                            self.spot(&s)
                          })
                        }
                    </table>
                </div>

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
    fn send_command(&mut self, cmd: Command) {
        let j = serde_json::to_string(&cmd).unwrap();
        let msg = format!("sent: {}", j);
        self.console.log(&msg);
        self.wss.send_with_str(&j).unwrap();
    }

    fn spot(&self, spot: &Spot) -> Html {
        let call = Call::new(spot.call.to_string());
        let country_icon =
            match call.country() {
                Ok(country) => html! { <i class=format!("flag-icon flag-icon-{}", country.code())></i> },
                Err(_) => html! {},
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
            </tr>
        }
    }

    fn radio(&self, radio: &Radio) -> Html {
        let radio_id = radio.id;
        html! {
            <div class="radio-control">
                { radio.name.to_string() }
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
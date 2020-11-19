#![recursion_limit = "2048"]
use anyhow::Error;
use wasm_bindgen::prelude::*;
use yew::services::{ConsoleService};
use yew::{html, Component, ComponentLink, Html, ShouldRender};
use yew_router::{service::RouteService};
use yew::format::{Json,Nothing};
use yew::services::interval::{IntervalService};
use yew::services::fetch::{FetchService, Request, Response};
use yew::services::reader::{ReaderService};
use yew::services::websocket::{WebSocketStatus};//, WebSocketTask};
use web_sys::{WebSocket,BinaryType,MessageEvent};
use wasm_bindgen::JsCast;
use std::time::Duration;
use adif;
use web_sys::{AudioBuffer, OfflineAudioContext};
use wasm_bindgen_futures::{spawn_local, JsFuture};
use std::cell::RefCell;
use std::rc::Rc;

use ham_rs::Call;
use ham_rs::countries::{CountryInfo,Country};
use ham_rs::rig::{Command,CommandResponse};
use ham_rs::log::LogEntry;

mod model;
use model::{Model,Msg,CallsignInfo,WebsocketMsgType};

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
            Msg::ReceivedAudio(data) => {
                // Both the commented out code and what is below are broken in different ways
                // The uncommented code will play the incoming data immedietly on top of what
                // is currently playing, The commented out code will _replace_ what is currently
                // playing with the incoming data.
                //
                // TODO: need to sequence the incoming data to play immedietly after previous
                // data finishes playing.
                //

                //let moved_buffer = self.buffer.clone();
                //let moved_source = self.source.clone();

                let moved_context = self.audio_ctx.clone();
                
                spawn_local(async move {
                    Some(async move {
                        let buffer = JsFuture::from(moved_context.decode_audio_data(&data)?)
                                .await?
                                .dyn_into::<AudioBuffer>();

                        let moved_buffer = buffer.clone();
                        match moved_buffer {
                            Ok(moved_buffer) => {
                                let source = moved_context.create_buffer_source().unwrap();
                                source.set_buffer(Some(&moved_buffer));
                                let destination = moved_context.destination();
                                source.connect_with_audio_node(&destination).unwrap();
                                source.set_loop(false);
                                source.start().unwrap();
                            },
                            Err(err) => {
                                ConsoleService::new().log(&format!("audo buffer error: {:?}", err));
                            }
                        }
                        buffer
                    }.await.unwrap());
                });

                /*spawn_local(async move {
                    *moved_buffer.borrow_mut() = Some(async move {
                        //JsFuture::from(decode_audio(&data))
                        //    .await?
                        //    .dyn_into::<AudioBuffer>()
                        let buffer = JsFuture::from(moved_context.decode_audio_data(&data)?)
                            .await?
                            .dyn_into::<AudioBuffer>();

                        let moved_buffer = buffer.clone();
                        match moved_buffer {
                            Ok(moved_buffer) => {
                                // TODO: need some kind of buffer here to append the new
                                // audio data to instead of replacing it
                                ConsoleService::new().log("decoded audio. adding to buffer.");
                                //let source = moved_context.create_buffer_source().unwrap();
                                moved_source.set_buffer(Some(&moved_buffer));

                                /*let destination = moved_context.destination();
                                let gain = moved_context.create_gain().unwrap();
                                gain.gain().set_value(1.0);
                                gain.connect_with_audio_node(&destination).unwrap();
                                source.connect_with_audio_node(&gain).unwrap();

                                source.set_loop(false);
                                source.start().unwrap();*/
                            },
                            Err(err) => {
                                ConsoleService::new().log(&format!("audo buffer error: {:?}", err));
                            }
                        }

                        buffer
                    }.await.unwrap());
                });*/
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
        let buffer = Rc::new(RefCell::new(None));
        let audio_ctx = web_sys::AudioContext::new().unwrap();
        let source = audio_ctx.create_buffer_source().unwrap();

        let destination = audio_ctx.destination();
        let gain = audio_ctx.create_gain().unwrap();
        gain.gain().set_value(1.0);
        gain.connect_with_audio_node(&destination).unwrap();
        source.connect_with_audio_node(&gain).unwrap();

        let analyzer = audio_ctx.create_analyser().unwrap();
        analyzer.connect_with_audio_node(&destination).unwrap();

        source.set_loop(false);
        source.start().unwrap();

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
            callsigns: vec![],
            audio_ctx: audio_ctx,
            source: source,
            buffer: buffer,
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

#[wasm_bindgen]
pub fn decode_audio(buffer: &js_sys::ArrayBuffer) -> js_sys::Promise {
    // See: https://github.com/magenta/magenta-js/blob/master/music/src/core/audio_utils.ts#L78
    let context = match OfflineAudioContext::new_with_number_of_channels_and_length_and_sample_rate(
        1, 16000, 16000.0
    ) {
        Ok(c) => c,
        Err(e) => return js_sys::Promise::reject(&e)
    };
    return match context.decode_audio_data(buffer) {
        Ok(p) => p,
        Err(e) => return js_sys::Promise::reject(&e)
    };
}

#[wasm_bindgen(start)]
pub fn run_app() {
    //App::<Model>::new().mount_to_body();
    yew::start_app::<Model>();
}
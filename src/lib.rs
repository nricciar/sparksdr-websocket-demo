#![recursion_limit = "2048"]
use wasm_bindgen::prelude::*;
use yew::{html, Component, ComponentLink, Html, ShouldRender, InputData};
use yew::{events::KeyboardEvent};
use yew::services::{ConsoleService};
use wasm_bindgen::prelude::*;
use wasm_bindgen::JsCast;
use wasm_bindgen_futures::JsFuture;
use web_sys::{HtmlCanvasElement,CanvasRenderingContext2d,AudioBuffer};
use wasm_bindgen_futures::{spawn_local};

use ham_rs::rig::{Command,CommandResponse};

mod model;
use model::{Model,Msg,SpotFilter};

impl Component for Model {
    type Message = Msg;
    type Properties = ();

    fn update(&mut self, msg: Self::Message) -> ShouldRender {
        match msg {
            Msg::Connected => {
                // When we first connect to SparkSDR gather some basic information
                self.send_command(Command::GetReceivers);
                self.send_command(Command::GetRadios);
                self.send_command(Command::GetVersion);
                // Also subscribe to spots
                self.send_command(Command::SubscribeToSpots{ enable: true });
                false
            },
            Msg::CommandResponse(Ok(msg)) => {
                match msg {
                    // getReceiversResponse: update our receiver list
                    CommandResponse::Receivers { receivers } => {
                        self.set_receivers(receivers);
                    },
                    //  update our radio list
                    CommandResponse::Radios { radios } => {
                        self.set_radios(radios);
                    },
                    // getVersionResponse: update our version info
                    CommandResponse::Version(version) => {
                        self.set_version(version);
                    },
                    // spotResponse: new incoming spots
                    CommandResponse::Spots { spots } => {
                        let cq_only = self.cq_only();
                        for spot in spots {
                            if (cq_only && spot.msg.contains("CQ")) || !cq_only {
                                self.add_spot(spot);
                            }
                        }
                        self.trim_spots(100);
                    },
                    // ReceiverResponse: receiver updates (mode/frequency)
                    CommandResponse::ReceiverResponse{ id: receiver_id, frequency, mode } => {
                        self.update_receiver(receiver_id, mode, frequency);
                    }
                }
                true
            },
            Msg::EnableAudio => {
                match self.subscribed_audio {
                    Some(audio_channel) => {
                        self.send_command(Command::SubscribeToAudio{ rx_id: audio_channel, enable: false });
                        self.subscribed_audio = None;
                        ConsoleService::log("unsubscribed to audio");
                    },
                    None => {
                        match self.default_receiver() {
                            Some(receiver) => {
                                self.send_command(Command::SubscribeToAudio{ rx_id: receiver.id, enable: true });
                                self.subscribed_audio = Some(receiver.id);
                                ConsoleService::log(&format!("subscribed to audio channel: {}", receiver.id));
                            },
                            None => ()
                        }
                    }
                }
                false
            },
            Msg::ReceivedAudio(data) => {
                match (self.audio_ctx(), self.gain()) {
                    (Some(audio_ctx), Some(gain)) => {
                        if self.audio_pos == 0 {
                            self.audio_start_time = audio_ctx.current_time();
                        }
                        self.audio_pos += 1;

                        let audio_pos = self.audio_pos;
                        let start_time = self.audio_start_time;

                        spawn_local(async move {
                            let future = JsFuture::from(audio_ctx.decode_audio_data(&data.slice(5)).unwrap());
                            match future.await {
                                Ok(value) => {
                                    if let Ok(decoded) = value.dyn_into::<AudioBuffer>() {
                                        let source = audio_ctx.create_buffer_source().unwrap();
                                        source.set_buffer(Some(&decoded));
                                        source.connect_with_audio_node(&gain).unwrap();
                                        source.set_loop(false);
                                        let play_time = start_time as f64 + (audio_pos as f64 * 512.0 / 48000.0) + 0.1;
                                        source.start_with_when(play_time).unwrap();
                                    } else {
                                        ConsoleService::error("decoded audio not a valid audio buffer");
                                    }
                                },
                                Err(err) => {
                                    ConsoleService::error(&format!("unable to decode audio data: {:?}", err));
                                }
                            }
                        });
                    },
                    _ => ()
                }
                false
            },
            Msg::MuteUnmute => {
                self.toggle_mute();
                true
            },
            Msg::CommandResponse(Err(err)) => {
                ConsoleService::error(&format!("command response error: {}", err));
                false
            },
            Msg::CallsignInfoReady(Ok(call)) => {
                self.cache_callsign_info(call);
                true
            },
            Msg::CallsignInfoReady(Err(err)) => {
                ConsoleService::error(&format!("callsign info error: {}", err));
                false
            },
            Msg::SetDefaultReceiver(receiver_id) => {
                match self.subscribed_audio {
                    Some(previous_audio_channel) => {
                        self.send_command(Command::SubscribeToAudio{ rx_id: previous_audio_channel, enable: false });
                        self.send_command(Command::SubscribeToAudio{ rx_id: receiver_id, enable: true });
                        self.subscribed_audio = Some(receiver_id);
                    },
                    None => ()
                }
                self.set_default_receiver(Some(receiver_id));
                true
            },
            Msg::AddReceiver(radio_id) => {
                self.send_command(Command::AddReceiver { id: radio_id });
                false
            },
            Msg::RemoveReceiver(receiver_id) => {
                self.send_command(Command::RemoveReceiver{ id: receiver_id });
                false
            },
            Msg::Tick => { // self.enable_ticks(seconds)
                /*let mut data = vec![0; self.analyser.frequency_bin_count() as usize];
                self.analyser.get_byte_frequency_data(&mut data);
                self.console.log(&format!("{:?}", data));

                match &self.canvas {
                    Some(canvas) => {
                        self.console.log(&format!("found canvas: {:?}", canvas));
                        let ctx = canvas.get_context("2d").unwrap().unwrap().dyn_into::<web_sys::CanvasRenderingContext2d>().unwrap();
                        for (i,d) in data.iter().enumerate() {
                            ctx.set_fill_style(&format!("rgb({}, 10, 10)", d).into());
                            ctx.fill_rect(i as f64, 0 as f64, 1 as f64, 1 as f64);
                        }
                        ctx.translate(0 as f64,1 as f64).unwrap();
                        ctx.set_transform(1 as f64, 0 as f64, 0 as f64, 1 as f64, 0 as f64, 0 as f64).unwrap();
                    },
                    None => {
                        self.console.log("unable to find canvas");
                    }
                }*/
                true
            },
            Msg::TogglePower(radio_id) => {
                match self.get_radio_power_state(radio_id) {
                    Some(state) => {
                        self.send_command(Command::SetRunning{ id: radio_id, running: !state });
                    },
                    None => {
                        ConsoleService::error(&format!("TogglePower: No radio found: {}", radio_id));
                    }
                }
                false
            },
            Msg::ToggleReceiverList => {
                self.toggle_receiver_list();
                true
            },
            Msg::ModeChanged(receiver_id, mode) => {
                self.change_receiver_mode(receiver_id, mode);
                true
            },
            Msg::FrequencyDown(receiver_id, digit) => {
                self.frequency_down(receiver_id, digit);
                true
            },
            Msg::FrequencyUp(receiver_id, digit) => {
                self.frequency_up(receiver_id, digit);
                true
            },
            Msg::Connect => {
                let addr = self.ws_location.to_string();
                ConsoleService::log(&format!("Connecting to {}", addr));
                self.connect(&addr);
                true
            },
            Msg::UpdateWebsocketAddress(address) => {
                self.ws_location = address;
                true
            },
            Msg::Disconnected => {
                self.disconnect();
                ConsoleService::error("Disconnected");
                true
            },
            Msg::SetGain(gain) => {
                self.set_gain(gain);
                true
            },
            Msg::RouteChanged(route) => {
                self.route = route;
                true
            },
            Msg::ChangeRoute(route) => {
                // This might be derived in the future
                self.route = route.into();
                self.route_service.set_route(&self.route.route, ());
                true
            },
            Msg::CancelImport => {
                self.clear_adif_data();
                true
            },
            Msg::ConfirmImport => {
                true
            },
            Msg::Loaded(data) => {
                self.load_adif_data(data);
                true
            },
            Msg::Files(files, _) => {
                for file in files.into_iter() {
                    self.read_file(file);
                }
                true
            },
            Msg::ToggleCQSpotFilter => {
                match self.cq_only() {
                    true => self.remove_filter(SpotFilter::CQOnly).unwrap(),
                    false => self.add_filter(SpotFilter::CQOnly),
                }
                true
            }
            Msg::None => { false }
        }
    }

    fn create(_props: Self::Properties, link: ComponentLink<Self>) -> Self {
        let mut model = Model::new(link);
        model.connect("ws://localhost:4649/Spark");
        // emit Msg::Tick every 10 seconds
        //model.enable_ticks(1);
        model
    }

    fn change(&mut self, _: Self::Properties) -> ShouldRender {
        true
    }

    fn rendered(&mut self, first_render: bool) {
        let canvas = self.node_ref.cast::<HtmlCanvasElement>().unwrap();
        self.canvas = Some(canvas);

        if first_render {
            self.initialize_audio();
        }
    }

    fn view(&self) -> Html {
        html! {
            <>
            {
                if self.is_connected() {
                    html! {
                        <>
                            { self.radio_list_control() }

                            <div class="control-bar">
                                { self.toggle_receivers_button() }
                                { self.enable_audio_button() }
                            </div>

                            <div style="clear:both"></div>

                            { self.receiver_list_control() }

                            <div style="clear:both"></div>
                            <canvas ref=self.node_ref.clone() width="512" height="300" style="display: none; background-color: black ;" />

                            { self.spots_view() }
                        </>
                    }
                } else {
                    html! {
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
                    }
                }
            }
            <div class="copy">
                <div class=if self.is_connected() { "" } else { "container" }>
                    { self.version_html() }
                    <p><a href="https://github.com/nricciar/sparksdr-websocket-demo" target="_blank">{ "sparksdr-websocket-demo @ github" }</a></p>
                </div>
            </div>
            </>
        }
    }
}

#[wasm_bindgen(start)]
pub fn run_app() {
    //App::<Model>::new().mount_to_body();
    yew::start_app::<Model>();
}
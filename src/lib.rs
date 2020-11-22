#![recursion_limit = "2048"]
use wasm_bindgen::prelude::*;
use yew::{html, Component, ComponentLink, Html, ShouldRender, InputData};
use yew::{events::KeyboardEvent, Classes};

use ham_rs::rig::{Command,CommandResponse};

mod model;
use model::{Model,Msg};

impl Component for Model {
    type Message = Msg;
    type Properties = ();

    fn update(&mut self, msg: Self::Message) -> ShouldRender {
        match msg {
            Msg::CommandResponse(Ok(msg)) => {
                match msg {
                    // getReceiversResponse: update our receiver list
                    CommandResponse::Receivers { Receivers: receivers } => {
                        self.set_receivers(receivers);
                    },
                    // getRadioResponse: update our radio list
                    CommandResponse::Radios { Radios: radios } => {
                        self.set_radios(radios);
                    },
                    // getVersionResponse: update our version info
                    CommandResponse::Version(version) => {
                        self.set_version(version);
                    },
                    // spotResponse: new incoming spots
                    CommandResponse::Spots { Spots: spots } => {
                        for spot in spots {
                            self.add_spot(spot);
                        }
                        self.trim_spots(100);
                    },
                    CommandResponse::ReceiverResponse{ ID: receiver_id, Frequency: frequency, Mode: mode } => {
                        self.update_receiver(receiver_id, mode, frequency);
                    }
                }
            },
            Msg::CommandResponse(Err(err)) => {
                let err = format!("error: {}", err);
                self.console.log(&err);
            },
            Msg::CallsignInfoReady(Ok(call)) => {
                self.cache_callsign_info(call);
            },
            Msg::CallsignInfoReady(Err(err)) => {
                let msg = format!("callsign info error: {}", err);
                self.console.log(&msg);
            },
            Msg::SetDefaultReceiver(receiver_id) => {
                self.set_default_receiver(Some(receiver_id));
            },
            Msg::AddReceiver(radio_id) => {
                self.send_command(Command::AddReceiver { ID: radio_id });
            },
            Msg::RemoveReceiver(receiver_id) => {
                self.send_command(Command::RemoveReceiver{ ID: receiver_id });
            },
            Msg::Tick => {
                self.send_command(Command::GetReceivers);
            },
            Msg::TogglePower(radio_id) => {
                match self.get_radio_power_state(radio_id) {
                    Some(state) => {
                        self.send_command(Command::SetRunning{ ID: radio_id, Running: !state });
                    },
                    None => {
                        self.console.log(&format!("TogglePower: No radio found: {}", radio_id));
                    }
                }
            },
            Msg::ToggleReceiverList => {
                self.toggle_receiver_list();
            },
            Msg::ModeChanged(receiver_id, mode) => {
                self.change_receiver_mode(receiver_id, mode);
            },
            Msg::FrequencyDown(receiver_id, digit) => {
                self.frequency_down(receiver_id, digit);
            },
            Msg::FrequencyUp(receiver_id, digit) => {
                self.frequency_up(receiver_id, digit);
            },
            Msg::Connect => {
                let addr = self.ws_location.to_string();
                self.console.log(&format!("Connecting to {}", addr));
                self.connect(&addr);
            },
            Msg::UpdateWebsocketAddress(address) => {
                self.ws_location = address;
            },
            Msg::Disconnected => {
                self.disconnect();
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
                self.handle_audio_data(data);
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
                self.clear_adif_data();
            },
            Msg::ConfirmImport => {
            },
            Msg::Loaded(data) => {
                self.load_adif_data(data);
            },
            Msg::Files(files, _) => {
                for file in files.into_iter() {
                    self.read_file(file);
                }
            },
            Msg::None => {}
        }
        true
    }

    fn create(_props: Self::Properties, link: ComponentLink<Self>) -> Self {
        let mut model = Model::new(link);
        model.connect("ws://localhost:4649/Spark");
        // emit Msg::Tick every 10 seconds
        //model.enable_ticks(10);
        model
    }

    fn change(&mut self, _: Self::Properties) -> ShouldRender {
        true
    }

    fn view(&self) -> Html {
        if self.is_connected() {
            html! {
                <>
                    { self.radio_list() }

                    <div class="control-bar">
                        { self.toggle_receivers() }
                    </div>

                    <div style="clear:both"></div>

                    { self.receiver_list() }

                    { self.spots_view() }

                    { self.version_html() }
                </>
            }
        } else {
            html! {
                <div class="container">
                    <h1 class="title">{ "Disconnected" }</h1>
                    <p>{ "Make sure SparkSDR has Web Sockets enabled, and hostname is correct "}</p>
                    <div class="field is-grouped" style="width:30em">
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
}

#[wasm_bindgen(start)]
pub fn run_app() {
    //App::<Model>::new().mount_to_body();
    yew::start_app::<Model>();
}
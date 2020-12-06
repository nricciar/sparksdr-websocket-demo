#![recursion_limit = "2048"]
#[macro_use]
extern crate serde_derive;
extern crate serde_json;
use wasm_bindgen::prelude::*;
use yew::{html, Component, ComponentLink, Html, ShouldRender};
use yew::services::{ConsoleService};
use yew_router::{Switch};
use web_sys::{HtmlCanvasElement};
use js_sys::{DataView};

use ham_rs::lotw::LoTWStatus;
use sparkplug::{Command,CommandResponse};

mod model;
mod color;
mod spot;
mod audio;
mod spectrum;

use model::{Model,Msg,AppRoute};
use spot::{SpotFilter};

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

                // fetch the lotw users file
                if !self.spots.has_lotw_users() {
                    self.spots.fetch_lotw_users(&self.link);
                }
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
                        let cq_only = self.spots.cq_only_spot_filter_enabled();
                        for spot in spots {
                            if (cq_only && spot.is_cq()) || !cq_only {
                                let current_rx_pass =
                                    match self.default_receiver() {
                                        Some(receiver) if self.spots.current_receiver_spot_filter_enabled() && receiver.has_spots() => {
                                            if spot.current_rx(&receiver) { true } else { false }
                                        },
                                        _ => true,
                                    };

                                if current_rx_pass {
                                    self.spots.add_spot(&self.link, spot, &self.import);
                                }
                            }
                        }
                        self.spots.trim_spots(100);
                    },
                    // ReceiverResponse: receiver updates (mode/frequency)
                    CommandResponse::ReceiverResponse{ id: receiver_id, frequency, mode, filter_low, filter_high } => {
                        self.update_receiver(receiver_id, mode, frequency, filter_low, filter_high);
                    }
                }
                true
            },
            Msg::EnableAudio => {
                match self.audio.receiving_audio() {
                    Some(_) => {
                        self.unsubscribe_to_audio();
                    },
                    None => {
                        self.subscribe_to_audio();
                    }
                }
                true
            },
            Msg::ReceivedAudio(data) => {
                let view = DataView::new(&data, 0, data.byte_length() as usize);
                let data_type = view.get_uint8(0);
                let receiver_id = view.get_int32(1);

                match (data_type, self.audio.receiving_audio(), self.spectrum.receiving_spectrum()) {
                    (1, Some(_), _) => {
                        self.audio.import_audio_data(data);
                    },
                    (2, _, Some(subscribed_spectrum)) => {// if subscribed_spectrum == (receiver_id as u32) => {
                        match self.default_receiver() {
                            Some(receiver) => {
                                let freq_start = view.get_float64_endian(5, true).floor();
                                let freq_stop = view.get_float64_endian(13, true).floor();
                                self.spectrum.import_spectrum_data(data, freq_start, freq_stop);
                            },
                            None => () // should never happen
                        }
                    },
                    (2, _, Some(_)) => (),
                    (_, None, None) => {
                        ConsoleService::error("receiving binary data but not subscribed to anything");
                    },
                    (dt, _, _) => {
                        ConsoleService::error(&format!("unsupported data type: {}", dt));
                    }
                }
                false
            },
            Msg::MuteUnmute => {
                self.audio.toggle_mute();
                true
            },
            Msg::CommandResponse(Err(err)) => {
                ConsoleService::error(&format!("command response error: {}", err));
                false
            },
            Msg::CallsignInfoReady(Ok(call)) => {
                // FIXME: json serialization issue
                let mut call = call;
                match call.lotw() {
                    LoTWStatus::Unknown => {
                        call.set_lotw(LoTWStatus::Unregistered);
                    },
                    _ => ()
                }

                self.spots.cache_callsign_info(call, &self.import);
                true
            },
            Msg::CallsignInfoReady(Err(err)) => {
                ConsoleService::error(&format!("callsign info error: {}", err));
                false
            },
            Msg::ClearSpots => {
                self.spots.clear_spots();
                true
            },
            Msg::SetDefaultReceiver(receiver_id) => {
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
                self.audio.set_gain(gain);
                true
            },
            Msg::RouteChanged(route) => {
                self.hide_receiver_list();
                self.route = route;
                true
            },
            Msg::ChangeRoute(route) => {
                self.hide_receiver_list();
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
            Msg::LotwUsers(users) => {
                ConsoleService::log("lotw users imported");
                self.spots.import_lotw_users(users);
                true
            },
            Msg::StatesOverlay(geo_json) => {
                ConsoleService::log("states overlay imported");
                self.spots.import_states_overlay(geo_json);
                false
            },
            Msg::ToggleCQSpotFilter => {
                match self.spots.cq_only_spot_filter_enabled() {
                    true => self.spots.remove_filter(SpotFilter::CQOnly).unwrap(),
                    false => self.spots.add_filter(SpotFilter::CQOnly),
                }
                true
            },
            Msg::ToggleCountrySpotFilter => {
                match self.spots.country_spot_filter_enabled() {
                    true => self.spots.remove_filter(SpotFilter::NewCountry).unwrap(),
                    false => self.spots.add_filter(SpotFilter::NewCountry),
                }
                true
            },
            Msg::ToggleStateSpotFilter => {
                match self.spots.state_spot_filter_enabled() {
                    true => self.spots.remove_filter(SpotFilter::NewState).unwrap(),
                    false => {
                        self.spots.add_filter(SpotFilter::NewState);

                        match self.spots.has_states_overlay() {
                            false => {
                                self.spots.fetch_states_overlay(&self.link);
                            },
                            true => ()
                        }
                    },
                }
                self.spots.update_states_overlay_js();
                true
            },
            Msg::ToggleCurrentReceiverSpotFilter => {
                match self.spots.current_receiver_spot_filter_enabled() {
                    true => self.spots.remove_filter(SpotFilter::CurrentReceiver).unwrap(),
                    false => self.spots.add_filter(SpotFilter::CurrentReceiver),
                }
                true
            },
            Msg::ToggleLoTWSpotFilter => {
                match self.spots.lotw_spot_filter_enabled() {
                    true => self.spots.remove_filter(SpotFilter::LoTW).unwrap(),
                    false => self.spots.add_filter(SpotFilter::LoTW),
                }
                true
            }
            Msg::None => { false }
        }
    }

    fn create(_props: Self::Properties, link: ComponentLink<Self>) -> Self {
        let mut model = Model::new(link);
        model.connect("ws://localhost:4649/Spark");
        model
    }

    fn change(&mut self, _: Self::Properties) -> ShouldRender {
        true
    }

    fn rendered(&mut self, first_render: bool) {
        let canvas = self.spectrum.canvas_node_ref.cast::<HtmlCanvasElement>().unwrap();
        self.spectrum.canvas = Some(canvas);

        let tmp_canvas = self.spectrum.tmp_canvas_node_ref.cast::<HtmlCanvasElement>().unwrap();
        self.spectrum.tmp_canvas = Some(tmp_canvas);

        if first_render {
            self.audio.create_audio_context();
            js_sys::eval("initMap();").unwrap();
        }
    }

    fn view(&self) -> Html {
        let (is_index, spectrum_style, map_style) =
            match AppRoute::switch(self.route.clone()) {
                Some(AppRoute::Index) | None => (true, "position:relative;margin-top:10px", "height:0px;overflow:hidden;"),
                _ => (false, "height:110px;overflow:hidden;position:relative;margin-top:10px", ""),
            };

        match self.is_connected() {
            false => self.disconnected_view(),
            true => {
                html! {
                    <>
                        { self.navbar_view() }

                        <div style="clear:both"></div>

                        { self.receiver_list_control() }
                        { self.spot_filters_sidebar() }

                        <div style="margin-left:15em;padding:0 10px 0 20px">
                            <div style=spectrum_style>
                                <div id="receiver-marker" style="display:none">
                                    <div></div>
                                </div>
                                <canvas id="waterfall" ref=self.spectrum.canvas_node_ref.clone() width="2048" height="200" style="position:relative;width:100%;height:200px;background-color: black" />
                            </div>
                            {
                                if is_index {
                                    html! {
                                        <>
                                            <table style="width:100%;border-left:2px solid #555;border-right:2px solid #555">
                                                <tr>
                                                    <th style="padding-left:10px" id="freq-start">{ self.spectrum.freq_start() }</th>
                                                    <th style="text-align:right;padding-right:10px" id="freq-end">{ self.spectrum.freq_stop() }</th>
                                                </tr>
                                            </table>
                                            { self.spots_view() }
                                        </>
                                    }
                                } else {
                                    html! { }
                                }
                            }
                            <div style=map_style>
                                <div id="map" style="width:100%;height:600px" class="has-background-light"> </div>
                            </div>
                        </div>

                        <canvas ref=self.spectrum.tmp_canvas_node_ref.clone() width="2048" height="200" style="display:none;background-color: black ;" />

                        { self.footer_view() }
                    </>
                }
            }
        }
    }
}

#[wasm_bindgen(start)]
pub fn run_app() {
    //App::<Model>::new().mount_to_body();
    yew::start_app::<Model>();
}
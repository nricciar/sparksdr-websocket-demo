use anyhow::Error;
use serde::{Deserialize};
use yew::services::fetch::{FetchTask};
use yew::{ComponentLink};
use yew::format::{Json,Text,Nothing};
use yew::services::fetch::{FetchService, Request, Response};
use yew::services::storage::{Area, StorageService};
use yew::services::{ConsoleService};
use std::collections::HashMap;

use ham_rs::{Call,CountryInfo,Country,LogEntry,Band};
use ham_rs::lotw::LoTWStatus;
use sparkplug::Spot;

use crate::model::{Model,Msg};

const FILTERS_KEY: &str = "radio.spots.filters";
const LOTW_USERS_KEY: &str = "radio.spots.lotwUsers";
const STATES_OVERLAY_KEY: &str = "radio.spots.statesOverlay";

#[derive(Debug, Serialize, Deserialize)]
enum LoTWUsers {
    Disabled,
    Users(String)
}

#[derive(Debug, Serialize, Deserialize)]
enum StatesOverlay {
    Disabled,
    GeoJson(String)
}

pub struct SpotDB {
    storage: StorageService,
    // Spots from enabling SubscribeToSpots
    spots: Vec<Spot>,
    pending_spots: HashMap<String,Vec<Spot>>,
    spot_filters: Vec<SpotFilter>,
    // Local callsign cache
    callsigns: HashMap<String,CallsignInfo>,
    lotw_ft: Option<FetchTask>,
    lotw_users: LoTWUsers,
    states_ft: Option<FetchTask>,
    states_overlay: StatesOverlay
}

impl SpotDB {
    pub fn new() -> SpotDB {
        let storage = StorageService::new(Area::Local).expect("storage was disabled by the user");
        let spot_filters = {
            if let Json(Ok(filters)) = storage.restore(FILTERS_KEY) {
                filters
            } else {
                Vec::new()
            }
        };
        let lotw_users = {
            if let Json(Ok(entries)) = storage.restore(LOTW_USERS_KEY) {
                ConsoleService::log("Restoring LoTW users file");
                entries
            } else {
                LoTWUsers::Disabled
            }
        };
        let states_overlay = {
            if let Json(Ok(entries)) = storage.restore(STATES_OVERLAY_KEY) {
                ConsoleService::log("Restoring states overlay");
                entries
            } else {
                StatesOverlay::Disabled
            }
        };

        SpotDB {
            storage,
            spots: Vec::new(),
            pending_spots: HashMap::new(),
            spot_filters: spot_filters,
            callsigns: HashMap::new(),
            lotw_ft: None,
            lotw_users: lotw_users,
            states_ft: None,
            states_overlay: states_overlay
        }
    }

    pub fn clear_spots(&mut self) {
        self.spots = Vec::new();
        self.pending_spots = HashMap::new();
    }

    pub fn spots(&self) -> &Vec<Spot> {
        &self.spots
    }

    pub fn has_lotw_users(&self) -> bool {
        match self.lotw_users {
            LoTWUsers::Users(_) => true,
            LoTWUsers::Disabled => false,
        }
    }

    pub fn import_lotw_users(&mut self, data: String) {
        self.lotw_users = LoTWUsers::Users(data);
        self.lotw_ft = None;
        self.storage.store(LOTW_USERS_KEY, Json(&self.lotw_users));
    }

    pub fn fetch_lotw_users(&mut self, link: &ComponentLink<Model>) {
        let callback = link.callback(
            move |response: Response<Text>| {
                let (meta, data) = response.into_parts();
                match data {
                    Ok(data) if meta.status.is_success() => {
                        Msg::LotwUsers(data)
                    },
                    _ => {
                        ConsoleService::error("unable to fetch lotw users list");
                        Msg::None
                    }
                }
            },
        );

        ConsoleService::log("requesting lotw users file");
        let request = Request::get("/out/lotw-users.dat").body(Nothing).unwrap();
        let ft = FetchService::fetch(request, callback).unwrap();            
        self.lotw_ft = Some(ft);
    }

    pub fn has_states_overlay(&self) -> bool {
        match self.states_overlay {
            StatesOverlay::GeoJson(_) => true,
            StatesOverlay::Disabled => false,
        }
    }

    pub fn update_states_overlay_js(&self) {
        let js =
            match &self.states_overlay {
                StatesOverlay::GeoJson(overlay) if self.state_spot_filter_enabled() => format!("statesOverlay = {};statesHidden = false;updateStateOverlay();", overlay),
                StatesOverlay::GeoJson(overlay) => format!("statesOverlay = {};statesHidden = true;updateStateOverlay();", overlay),
                StatesOverlay::Disabled => format!("statesOverlay = null;statesHidden = true;updateStateOverlay();"),
            };
        
        js_sys::eval(&js).unwrap();
    }

    pub fn import_states_overlay(&mut self, data: String) {
        self.states_overlay = StatesOverlay::GeoJson(data);
        self.states_ft = None;
        self.storage.store(STATES_OVERLAY_KEY, Json(&self.states_overlay));
        self.update_states_overlay_js();
    }

    pub fn fetch_states_overlay(&mut self, link: &ComponentLink<Model>) {
        let callback = link.callback(
            move |response: Response<Text>| {
                let (meta, data) = response.into_parts();
                match data {
                    Ok(data) if meta.status.is_success() => {
                        Msg::StatesOverlay(data)
                    },
                    _ => {
                        ConsoleService::error("unable to fetch sates overlay");
                        Msg::None
                    }
                }
            },
        );

        ConsoleService::log("requesting states overlay");
        let request = Request::get("/out/states.json").body(Nothing).unwrap();
        let ft = FetchService::fetch(request, callback).unwrap();            
        self.states_ft = Some(ft);
    }

    pub fn has_callsign_info(&mut self, call: &Call) -> Option<Call> {
        match self.callsigns.get(&call.call()) {
            Some(CallsignInfo::Found(call)) => Some(call.clone()),
            _ => {
                match call.country() {
                    Ok(country) if country == Country::UnitedStates => None,
                    _ => {
                        match &self.lotw_users {
                            LoTWUsers::Users(users) => {
                                let mut call = call.clone();
                                if users.contains(&call.call()) {
                                    call.set_lotw(LoTWStatus::Registered);
                                } else {
                                    call.set_lotw(LoTWStatus::Unregistered);
                                }
                                self.callsigns.insert(call.call(), CallsignInfo::Found(call.clone()));
                                Some(call)
                            },
                            LoTWUsers::Disabled => None,
                        }
                    }
                }
            }
        }
    }

    // CommandResponse: spotResponse
    pub fn add_spot(&mut self, link: &ComponentLink<Model>, spot: Spot, logs: &Option<Vec<LogEntry>>) {
        // FIXME: temp fix
        let mut spot = spot;

        let pending =
            match self.has_callsign_info(&spot.call) {
                Some(call) => {
                    spot.set_call(call);
                    false
                },
                None => {
                    // If a US callsign attempt to fetch additonal callsign
                    // info from server otherwise we are done
                    match CallsignInfo::fetch(link, &spot.call) {
                        Some(ft) => {
                            self.callsigns.insert(spot.call.call(), ft);
                            true
                        },
                        None => false,
                    }
                }
            };

        match pending {
            true => {
                self.pending_spots.entry(spot.call.call()).or_insert(Vec::new()).push(spot);
            },
            false => self.internal_spot_push(spot, logs),
        }
    }

    fn internal_spot_push(&mut self, spot: Spot, logs: &Option<Vec<LogEntry>>) {
        match (logs, self.state_spot_filter_enabled(), self.country_spot_filter_enabled()) {
            (Some(logs), true, false) if !spot.new_state(logs) => (),
            (Some(logs), false, true) if !spot.new_country(logs) => (),
            (Some(logs), true, true) if !spot.new_country(logs) || !spot.new_state(logs) => (),
            _ => {
                match self.lotw_spot_filter_enabled() {
                    true if !spot.uses_lotw() => (),
                    _ => {
                        match &spot.locator {
                            Some(locator) => {
                                match locator.coord() {
                                    Ok((lat,lon)) => {
                                        let spot_on = spot.time.format("%H%M%S").to_string();
                                        let band = Band::new(spot.tuned_frequency as i32);
                                        let uses_lotw =
                                            match spot.call.lotw() {
                                                LoTWStatus::Registered | LoTWStatus::LastUpload(_) => true,
                                                _ => false,
                                            };
                                        let is_cq = spot.is_cq();
                                        match band.band() {
                                            Some(band_name) => {
                                                js_sys::eval(&format!("addMarker(\"{}\", {}, {}, \"{}\", {}, \"{}\", {}, {}, \"{}\");", spot.call.call(), lat, lon, spot_on, spot.tuned_frequency, band_name, uses_lotw, is_cq, spot.mode.mode())).unwrap();
                                            },
                                            _ => (),
                                        }
                                    },
                                    Err(_) => (),
                                }
                            },
                            None => (),
                        }
                        self.spots.push(spot)
                    }
                }
            }
        }
    }

    // helper function to remove all except `limit` recent spots
    pub fn trim_spots(&mut self, limit: usize) {
        if self.spots.len() > limit {
            let drain = self.spots.len() - limit;
            self.spots.drain(0..drain);
        }
    }

    pub fn cache_callsign_info(&mut self, call: Call, logs: &Option<Vec<LogEntry>>) {
        self.callsigns.insert(call.call(), CallsignInfo::Found(call.clone()));

        // remove spots from pending queue and publish them
        // with callsign info
        match self.pending_spots.remove(&call.call()) {
            Some(mut spots) => {
                for mut spot in spots.drain(..) {
                    spot.set_call(call.clone());
                    self.internal_spot_push(spot, logs);
                }
            },
            None => ()
        }
    }

    pub fn add_filter(&mut self, filter: SpotFilter) {
        self.spot_filters.push(filter);
        self.spot_filters.sort();
        self.spot_filters.dedup();
        self.storage.store(FILTERS_KEY, Json(&self.spot_filters));
    }

    pub fn remove_filter(&mut self, filter:SpotFilter) -> Result<(),&'static str> {
        match self.spot_filters.iter().position(|x| *x == filter) {
            Some(index) => {
                self.spot_filters.remove(index);
                self.storage.store(FILTERS_KEY, Json(&self.spot_filters));
                Ok(())
            },
            None => Err("not found")
        }
    }
    
    pub fn cq_only_spot_filter_enabled(&self) -> bool {
        self.spot_filters.iter().any(|s| match s {
            SpotFilter::CQOnly => true,
            _ => false,
        })
    }

    pub fn state_spot_filter_enabled(&self) -> bool {
        self.spot_filters.iter().any(|s| match s {
            SpotFilter::NewState => true,
            _ => false,
        })
    }

    pub fn country_spot_filter_enabled(&self) -> bool {
        self.spot_filters.iter().any(|s| match s {
            SpotFilter::NewCountry => true,
            _ => false,
        })
    }

    pub fn current_receiver_spot_filter_enabled(&self) -> bool {
        self.spot_filters.iter().any(|s| match s {
            SpotFilter::CurrentReceiver => true,
            _ => false,
        })
    }

    pub fn lotw_spot_filter_enabled(&self) -> bool {
        self.spot_filters.iter().any(|s| match s {
            SpotFilter::LoTW => true,
            _ => false,
        })
    }
}

#[derive(Ord, PartialOrd, Eq, PartialEq, Deserialize, Serialize)]
pub enum SpotFilter {
    CQOnly,
    NewState,
    NewCountry,
    CurrentReceiver,
    LoTW,
}



// Used with the local callsign cache for our requests
// for callsign info.
pub enum CallsignInfo {
    Requested((Call, FetchTask)),
    Found(Call),
    NotFound(Call)
}

impl CallsignInfo {
    pub fn fetch(link: &ComponentLink<Model>, call: &Call) -> Option<CallsignInfo> {
        let callback = link.callback(
            move |response: Response<Json<Result<Call, Error>>>| {
                let (meta, Json(data)) = response.into_parts();
                if meta.status.is_success() {
                    Msg::CallsignInfoReady(data)
                } else {
                    Msg::None
                }
            },
        );

        match (call.country(), call.prefix()) {
            (Ok(country), Some(prefix)) if country == Country::UnitedStates => {
                let request = Request::get(format!("/out/{}/{}.json", prefix, call.call())).body(Nothing).unwrap();
                let ft = FetchService::fetch(request, callback).unwrap();
                Some(CallsignInfo::Requested((call.clone(), ft)))
            },
            _ => None,
        }
    }

    pub fn call(&self) -> Call {
        match self {
            CallsignInfo::Requested((c, _)) => c.clone(),
            CallsignInfo::Found(c) => c.clone(),
            CallsignInfo::NotFound(c) => c.clone(),
        }
    }
}
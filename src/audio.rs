use web_sys::{AudioContext, GainNode};
use yew::services::{ConsoleService};
use wasm_bindgen::JsCast;
use wasm_bindgen_futures::{spawn_local};
use wasm_bindgen_futures::JsFuture;
use web_sys::{AudioBuffer};

pub struct AudioProvider {
    // audio playback
    audio_ctx: Option<AudioContext>,
    gain: Option<GainNode>,
    //pub analyser: AnalyserNode,
    audio_pos: u64,
    audio_start_time: f64,
    subscribed_audio: Option<u32>,
}

impl AudioProvider {
    pub fn new() -> AudioProvider {
        AudioProvider {
            audio_ctx: None,
            gain: None,
            audio_pos: 0,
            audio_start_time: 0.0,
            subscribed_audio: None,
        }
    }

    pub fn receiving_audio(&self) -> Option<u32> {
        self.subscribed_audio
    }

    pub fn set_subscribed(&mut self, receiver: Option<u32>) {
        self.subscribed_audio = receiver;
        match self.subscribed_audio {
            Some(_) => (),
            None => {
                self.subscribed_audio = None;
                self.audio_pos = 0;
                self.audio_start_time = 0.0;
            }
        }
    }

    pub fn create_audio_context(&mut self) {
        // audio channel
        let audio_ctx = web_sys::AudioContext::new().unwrap();
        let destination = audio_ctx.destination();

        //let analyser = audio_ctx.create_analyser().unwrap();
        //analyser.connect_with_audio_node(&destination).unwrap();

        let gain = audio_ctx.create_gain().unwrap();
        gain.gain().set_value(1.0);
        gain.connect_with_audio_node(&destination).unwrap();

        self.audio_ctx = Some(audio_ctx);
        self.gain = Some(gain);
    }

    pub fn import_audio_data(&mut self, data: js_sys::ArrayBuffer) {
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
    }

    pub fn audio_ctx(&self) -> Option<AudioContext> {
        match &self.audio_ctx {
            Some(audio_ctx) => Some(audio_ctx.clone()),
            None => None,
        }
    }

    pub fn gain(&self) -> Option<GainNode> {
        match &self.gain {
            Some(gain) => Some(gain.clone()),
            None => None,
        }
    }

    pub fn set_gain(&mut self, gain: f32) {
        if let Some(g) = &self.gain {
            g.gain().set_value(gain);
        }
    }

    pub fn toggle_mute(&mut self) {
        if let Some(g) = &self.gain {
            let value = g.gain().value();
            if value == 0.0 {
                g.gain().set_value(1.0);
                ConsoleService::log("unmuting audio");
            } else {
                g.gain().set_value(0.0);
                ConsoleService::log("muting audio");
            }
        }
    }


}
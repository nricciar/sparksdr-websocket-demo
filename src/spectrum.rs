use web_sys::{HtmlCanvasElement};
use yew::prelude::*;
use yew::services::{ConsoleService};
use wasm_bindgen::{JsCast,Clamped};
use web_sys::{ImageData};

use crate::color::{ColourGradient};

pub struct SpectrumProvider {
    pub canvas_node_ref: NodeRef,
    pub tmp_canvas_node_ref: NodeRef,
    pub canvas: Option<HtmlCanvasElement>,
    pub tmp_canvas: Option<HtmlCanvasElement>,
    subscribed_spectrum: Option<u32>,
    freq_start: f64,
    freq_stop: f64,
    spectrum_buffer: Vec<[f32;2048]>,
    gradient: ColourGradient,
}

impl SpectrumProvider {
    pub fn new() -> SpectrumProvider {
        let mut gradient = ColourGradient::new();
        gradient.set_min(0.0);
        gradient.set_max(255.0);

        SpectrumProvider {
            canvas_node_ref: NodeRef::default(),
            tmp_canvas_node_ref: NodeRef::default(),
            canvas: None,
            tmp_canvas: None,
            subscribed_spectrum: None,
            freq_start: 0.0,
            freq_stop: 0.0,
            spectrum_buffer: Vec::new(),
            gradient: gradient,
        }
    }

    pub fn freq_start(&self) -> f64 {
        self.freq_start
    }

    pub fn freq_stop(&self) -> f64 {
        self.freq_stop
    }

    pub fn receiving_spectrum(&self) -> Option<u32> {
        self.subscribed_spectrum
    }

    pub fn set_subscribed(&mut self, receiver: Option<u32>) {
        self.subscribed_spectrum = receiver;
    }

    pub fn import_spectrum_data(&mut self, data: js_sys::ArrayBuffer, start: f64, stop: f64) {
        let data = js_sys::Float32Array::new(&data.slice(1+4+8+8));
        let mut tmp = [0.0; 2048];
        data.copy_to(&mut tmp);
        self.spectrum_buffer.push(tmp);

        match (self.spectrum_buffer.len(), &self.canvas, &self.tmp_canvas) {
            (buffer_len, Some(canvas), Some(tmp_canvas)) if buffer_len >= 10 => {
                // TODO: move this somewhere
                if self.freq_start != start || self.freq_stop != stop {
                    let js = format!("frequencyStart = {};frequencyStop = {};updateWaterfallNav();", start, stop);
                    ConsoleService::log(&format!("js: {}", js));
                    js_sys::eval(&js).unwrap();
                    self.freq_stop = stop;
                    self.freq_start = start;
                }

                // canvas ctx
                let ctx = canvas.get_context("2d").unwrap().unwrap().dyn_into::<web_sys::CanvasRenderingContext2d>().unwrap();
                let tmp_ctx = tmp_canvas.get_context("2d").unwrap().unwrap().dyn_into::<web_sys::CanvasRenderingContext2d>().unwrap();

                // make copy of current canvas
                tmp_ctx.draw_image_with_html_canvas_element_and_dw_and_dh(&canvas, 0.0, 0.0, 2048.0, 200.0).unwrap();

                let mut line = vec![0; 8192];
                let mut iter = line.chunks_exact_mut(4);
                for i in 0..2047 {
                    // average pixel value over our buffer array
                    let mut max = self.spectrum_buffer.iter().max_by_key(|b| b[i] as u32 ).unwrap()[i] + 180.0;
                    if max > 255.0 { max = 255.0; }
                    if max < 0.0 { max = 0.0; }
                    let color = self.gradient.get_colour(max);

                    // Color to ImageData pixel
                    for pixel in iter.next() {
                        if let [r,g,b,a] = pixel {
                            *a = u8::max_value();
                            *r = color.r;
                            *g = color.g;
                            *b = color.b;
                        }
                    };
                }

                // add our new line to the canvas
                let line = ImageData::new_with_u8_clamped_array(Clamped(&mut line), 2048 ).unwrap();
                ctx.put_image_data(&line, 0.0, 0.0).unwrap();

                // waterfall scroll
                ctx.translate(0 as f64,1 as f64).unwrap();
                ctx.draw_image_with_html_canvas_element_and_sw_and_sh_and_dx_and_dy_and_dw_and_dh(&tmp_canvas, 0.0, 0.0, 2048.0, 200.0, 0.0, 0.0, 2048.0, 200.0).unwrap();
                ctx.set_transform(1.0, 0.0, 0.0, 1.0, 0.0, 0.0).unwrap();

                self.spectrum_buffer = Vec::new();
            },
            (_, None, _) |
            (_, _, None) => {
                ConsoleService::error("unable to find canvas");
            },
            _ => ()
        }
    }
}
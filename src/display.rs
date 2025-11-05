extern crate sdl2;

use std::iter::zip;

use crate::emulator::constants::*;
use sdl2::{
    EventPump,
    event::EventPollIterator,
    render::{Texture, TextureCreator, WindowCanvas},
    video::WindowContext,
};

// sdl2 more like sdlPOO
// my textures are unsafe now (yay!)
pub struct Display {
    canvas: WindowCanvas,
    event_pump: EventPump,
    #[allow(dead_code)]
    texture_creator: TextureCreator<WindowContext>,
    texture: Texture,
}

impl Display {
    pub fn new() -> Self {
        let sdl_context = sdl2::init().unwrap();
        let video_subsystem = sdl_context.video().unwrap();
        const SCALE: u32 = 4;
        let window = video_subsystem
            .window("Gameboy", SCRN_X as u32 * SCALE, SCRN_Y as u32 * SCALE)
            .position_centered()
            .build()
            .unwrap();
        let mut canvas = window.into_canvas().build().unwrap();
        canvas
            .set_logical_size(SCRN_X as u32, SCRN_Y as u32)
            .unwrap();
        let texture_creator = canvas.texture_creator();
        let texture = texture_creator
            .create_texture_streaming(None, SCRN_X as u32, SCRN_Y as u32)
            .unwrap();
        Display {
            canvas,
            event_pump: sdl_context.event_pump().unwrap(),
            texture_creator,
            texture,
        }
    }
    pub fn events(&mut self) -> EventPollIterator<'_> {
        self.event_pump.poll_iter()
    }
    pub fn update(&mut self, fb: &[u8; SCRN_X * SCRN_Y]) {
        self.texture
            .with_lock(None, |pixels, pitch| {
                for row in 0..SCRN_Y {
                    for (i, color) in zip(
                        ((row * pitch)..(row * pitch + SCRN_X * 4)).step_by(4),
                        fb.iter().skip(row * SCRN_X),
                    ) {
                        // this is in the lovely format BGRA
                        pixels[i..(i + 4)].copy_from_slice(match color {
                            0 => &[0x8C, 0xDE, 0xC6, 255],
                            1 => &[0x63, 0xA5, 0x84, 255],
                            2 => &[0x39, 0x61, 0x39, 255],
                            3 => &[0x10, 0x18, 0x08, 255],
                            _ => unreachable!(),
                        });
                    }
                }
            })
            .unwrap();
        self.canvas.clear();
        let _ = self.canvas.copy(&self.texture, None, None);
        self.canvas.present();
    }
    pub fn show(&mut self) {
        self.canvas.present();
    }
}

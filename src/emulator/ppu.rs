use std::cmp::Ordering;

use super::{Ram, constants::*};
use FetchState::*;
use Mode::*;
use arrayvec::ArrayVec;

struct Object {
    y: u8,
    x: u8,
    index: u8,
    flags: u8,
}

impl Ord for Object {
    fn cmp(&self, other: &Self) -> Ordering {
        if self.x != other.x {
            self.x.cmp(&other.x)
        } else {
            self.index.cmp(&other.index)
        }
    }
}

impl PartialOrd for Object {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl PartialEq for Object {
    fn eq(&self, other: &Self) -> bool {
        self.x == other.x && self.index == other.index
    }
}

impl Eq for Object {}

#[derive(Debug, PartialEq, Eq)]
enum FetchState {
    GetTile,
    GetTileDataLow,
    GetTileDataHigh,
    Push,
}

pub(super) struct Fetcher {
    pub(super) framebuffer: [u8; SCRN_X * SCRN_Y],
    x: u8,
    draw_x: u8,
    objects: ArrayVec<Object, 10>,
    bg_fifo: ArrayVec<u8, 8>,
    obj_fifo: ArrayVec<u8, 8>,
    state: FetchState,
    delay: u8,
    index: u8,
    tile: (u8, u8),
}

impl Fetcher {
    // TODO: window
    fn tick(&mut self, ram: &Ram) {
        if self.delay == 0 {
            self.tick_fetcher(ram);
        } else {
            self.delay -= 1;
        }
        if self.x == 0 || self.draw_x as usize >= SCRN_X {
            return;
        }
        let pixel = self.bg_fifo.pop().unwrap_or(0);
        self.framebuffer[ram.read(LY) as usize * SCRN_X + self.draw_x as usize] = pixel;
        self.draw_x += 1;
    }
    fn tick_fetcher(&mut self, ram: &Ram) {
        let ly = ram.read(LY);
        let lcdc = ram.read(LCDC);
        let scy = ram.read(SCY);
        let scx = ram.read(SCX);
        match self.state {
            GetTile => {
                let base = if lcdc & (1 << 3) == 0 { 0x9800 } else { 0x9C00 };
                let tile_x = (((scx + self.x) / 8) % 32) as u16;
                let tile_y = ((ly.wrapping_add(scy)) / 8) as u16;
                self.index = ram.read(base + tile_y * 32 + tile_x);
                self.state = GetTileDataLow;
                self.delay = 1;
            }
            GetTileDataLow => {
                let row = (ly.wrapping_add(scy)) % 8;
                let addr = 2 * row as u16
                    + if lcdc & (1 << 4) > 0 {
                        0x8000 + self.index as u16 * 16
                    } else {
                        (0x9000_u16 as i16).wrapping_add(self.index as i8 as i16 * 16) as u16
                    };
                self.tile.0 = ram.read(addr);
                self.state = GetTileDataHigh;
                self.delay = 1;
            }
            GetTileDataHigh => {
                let row = (ly.wrapping_add(scy)) % 8;
                let addr = 2 * row as u16
                    + if lcdc & (1 << 4) > 0 {
                        0x8000 + self.index as u16 * 16
                    } else {
                        (0x9000_u16 as i16).wrapping_add(self.index as i8 as i16 * 16) as u16
                    };
                self.tile.1 = ram.read(addr + 1);
                self.state = Push;
                self.delay = 1;
            }
            Push => {
                if self.bg_fifo.is_empty() {
                    let start = if self.x == 0 { scx % 8 } else { 0 };
                    for bit in start..8 {
                        let pixel = ((self.tile.0 >> bit) & 1) | (((self.tile.1 >> bit) & 1) << 1);
                        self.bg_fifo.push(pixel);
                        self.x += 1;
                    }
                    self.state = GetTile;
                }
            }
        }
    }
    fn reset(&mut self) {
        self.x = 0;
        self.draw_x = 0;
        self.bg_fifo.clear();
        self.obj_fifo.clear();
        self.state = GetTile;
        self.delay = 0;
    }
}

#[derive(PartialEq, Eq)]
pub(super) enum Mode {
    Mode0,
    Mode1,
    Mode2,
    Mode3,
}

pub struct Ppu {
    counter: u32,
    pub(super) mode: Mode,
    pub(super) fetcher: Fetcher,
}

impl Ppu {
    pub fn new() -> Self {
        Ppu {
            counter: 0,
            mode: Mode0,
            fetcher: Fetcher {
                framebuffer: [0; SCRN_X * SCRN_Y],
                x: 0,
                draw_x: 0,
                objects: ArrayVec::new(),
                bg_fifo: ArrayVec::new(),
                obj_fifo: ArrayVec::new(),
                state: GetTile,
                delay: 0,
                index: 0,
                tile: (0, 0),
            },
        }
    }
    // TODO: implement STAT
    pub fn tick(&mut self, ram: &mut Ram, dots: u8) {
        const SCANLINE_DOTS: u32 = 456;
        let lcdc = ram.read(LCDC);
        if lcdc & (1 << 7) == 0 {
            return;
        }
        let mut ly = ram.read(LY);
        for _ in 0..dots {
            match self.mode {
                Mode0 => {
                    self.counter += 1;
                    if self.counter == SCANLINE_DOTS {
                        self.counter = 0;
                        ly += 1;
                        if ly < 144 {
                            self.mode = Mode2;
                            self.oam_scan(ram);
                        } else {
                            self.mode = Mode1;
                            ram.write(IF, ram.read(IF) | 1);
                        }
                    }
                }
                Mode1 => {
                    self.counter += 1;
                    if self.counter == SCANLINE_DOTS {
                        self.counter = 0;
                        ly += 1;
                        if ly > 153 {
                            ly = 0;
                            self.mode = Mode2;
                            self.oam_scan(ram);
                        }
                    }
                }
                Mode2 => {
                    self.counter += 1;
                    if self.counter == 80 {
                        self.mode = Mode3;
                        self.fetcher.reset();
                        // self.draw_scanline(ram);
                    }
                }
                Mode3 => {
                    // self.counter += 1;
                    // if self.counter == 80 + 172 {
                    //     self.mode = Mode0;
                    // }
                    self.counter += 1;
                    self.fetcher.tick(ram);
                    if self.fetcher.x as usize >= SCRN_X {
                        self.mode = Mode0;
                    }
                }
            }
        }
        ram.write(LY, ly);
    }
    // TODO: window/objects
    // dot-accurate rendering
    fn _draw_scanline(&mut self, ram: &Ram) {
        let ly = ram.read(LY);
        let lcdc = ram.read(LCDC);
        let scy = ram.read(SCY);
        let scx = ram.read(SCX);
        let tile_row = (ly.wrapping_add(scy) / 8) as u16;
        let mut tile_col = (scx / 8) as u16;
        let base = if lcdc & (1 << 3) == 0 { 0x9800 } else { 0x9C00 };
        let row = (ly.wrapping_add(scy)) % 8;
        let method8000 = lcdc & (1 << 4) > 0;
        let mut next_tile = || {
            let index = ram.read(base + tile_row * 32 + tile_col);
            tile_col += 1;
            tile_col %= 32;
            let addr = 2 * row as u16
                + if method8000 {
                    0x8000 + index as u16 * 16
                } else {
                    (0x9000_u16 as i16).wrapping_add(index as i8 as i16 * 16) as u16
                };
            (ram.read(addr), ram.read(addr + 1))
        };
        let mut x = 0;
        let mut draw_tile = |bit_range: std::ops::Range<u8>| {
            let tile = next_tile();
            for bit in bit_range.rev() {
                let color = ((tile.0 >> bit) & 1) | (((tile.1 >> bit) & 1) << 1);
                self.fetcher.framebuffer[ly as usize * SCRN_X + x] = color;
                x += 1;
            }
        };
        // do first tile
        draw_tile((scx % 8)..8);
        // do every tile in-between
        for _ in 1..20 {
            draw_tile(0..8);
        }
        // do last tile
        draw_tile(0..(scx % 8));
    }
    fn oam_scan(&mut self, ram: &Ram) {
        self.fetcher.objects.clear();
        let ly = ram.read(LY);
        let lcdc = ram.read(LCDC);
        let obj_height = if lcdc & (1 << 2) > 0 { 16 } else { 8 };
        for i in (0xFE00..0xFEA0).step_by(4) {
            let y = ram.read(i);
            if (y..(y + obj_height)).contains(&ly) {
                self.fetcher.objects.push(Object {
                    y,
                    x: ram.read(i + 1),
                    index: ram.read(i + 2),
                    flags: ram.read(i + 3),
                });
                if self.fetcher.objects.is_full() {
                    break;
                }
            }
        }
        // sort in reverse order
        self.fetcher.objects.sort_by(|o1, o2| o2.cmp(o1));
    }
}

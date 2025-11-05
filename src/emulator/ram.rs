use std::io::{self, ErrorKind, Read};

use super::constants::*;

pub struct Ram {
    // mem goes from 0x0000 to 0xFFFF
    pub mem: [u8; 0x10000],
    // each bank has 16kb or rom
    banks: Vec<[u8; 0x4000]>,
    active_bank: usize,
}

pub trait CpuBus {
    fn read(&self, i: u16) -> u8;
    fn write(&mut self, i: u16, byte: u8);
}

impl CpuBus for Ram {
    fn read(&self, i: u16) -> u8 {
        // if i == LY {
        //     return 0x90;
        // }
        // bank 01-NN
        if (0x4000..0x8000).contains(&i) {
            return self.banks[self.active_bank][i as usize - 0x4000];
        }
        // echo ram
        if (0xE000..=0xFDFF).contains(&i) {
            return self.mem[i as usize - 0x2000];
        }
        self.mem[i as usize]
    }
    // TODO: support other kinds of mbc
    fn write(&mut self, i: u16, val: u8) {
        // ram enable
        if i < 0x2000 {
            panic!("RAM enable register not implemented!");
        }
        // rom bank number
        // FIX: blah blah some logic with only using as many bits as needed
        if i < 0x4000 {
            self.active_bank = match val & 0b11111 {
                0 => 0,
                // we sub 1 because bank 1 is at index 0
                b => b - 1,
            } as usize;
            return;
        }
        // ram bank number or upper bits of rom bank number
        if i < 0x6000 {
            panic!("RAM bank number register not implemented!");
        }
        // mode select
        if i < 0x8000 {
            panic!("Mode select register not implemented!");
        }
        // echo ram
        if (0xE000..0xFE00).contains(&i) {
            self.mem[i as usize - 0x2000] = val;
            return;
        }
        self.mem[i as usize] = val;
    }
}

impl Ram {
    pub fn new() -> Self {
        let mut mem = [0; 0x10000];
        // LCDC (0xFF40) defaults to 10010001 (0x91) meaning
        // bit 7 - LCD and PPU enabled
        // bit 6 - Window tile map area is 9800-9BFF
        // bit 5 - Window disabled
        // bit 4 - BG and Window tile data area is 8000-8FFF
        // bit 3 - BG tile map area is 9800-9BFF
        // bit 2 - Object size is 8x8
        // bit 1 - Objects are disabled
        // bit 0 - BG and Window are enabled (basically)
        mem[LCDC as usize] = 0x91;
        mem[IF as usize] = 0xE1;
        Ram {
            mem,
            banks: Vec::new(),
            active_bank: 0,
        }
    }
    pub fn read(&self, i: u16) -> u8 {
        // bank 01-NN
        if (0x4000..0x8000).contains(&i) {
            return self.banks[self.active_bank][i as usize - 0x4000];
        }
        // echo ram
        if (0xE000..=0xFDFF).contains(&i) {
            return self.mem[i as usize - 0x2000];
        }
        self.mem[i as usize]
    }
    pub fn write(&mut self, i: u16, val: u8) {
        // ram enable
        if i < 0x2000 {
            panic!("RAM enable register not implemented!");
        }
        // rom bank number
        // FIX: blah blah some logic with only using as many bits as needed
        if i < 0x4000 {
            self.active_bank = match val & 0b11111 {
                0 => 0,
                // we sub 1 because bank 1 is at index 0
                b => b - 1,
            } as usize;
            return;
        }
        if i == DMA {
            panic!("OAM DMA transfer not implemented!");
        }
        // ram bank number or upper bits of rom bank number
        if i < 0x6000 {
            panic!("RAM bank number register not implemented!");
        }
        // mode select
        if i < 0x8000 {
            panic!("Mode select register not implemented!");
        }
        // echo ram
        if (0xE000..0xFE00).contains(&i) {
            self.mem[i as usize - 0x2000] = val;
            return;
        }
        self.mem[i as usize] = val;
    }
    pub(super) fn load<R: Read>(&mut self, input: &mut R) -> io::Result<()> {
        input.read_exact(&mut self.mem[..0x4000])?;
        let mut buf = [0; 0x4000];
        loop {
            if let Err(e) = input.read_exact(&mut buf) {
                if e.kind() == ErrorKind::UnexpectedEof {
                    return Ok(());
                } else {
                    return Err(e);
                }
            }
            self.banks.push(buf);
        }
    }
}

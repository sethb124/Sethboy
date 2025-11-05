use std::{
    collections::HashSet,
    io::{self, Read, Write, stdin, stdout},
    process::exit,
};

use self::{constants::*, cpu::*, ppu::*, ram::*, timer::*};

pub mod constants;
mod cpu;
mod ppu;
mod ram;
mod timer;

pub struct Emulator {
    cpu: Cpu,
    ppu: Ppu,
    pub ram: Ram,
    timer: Timer,
    breakpoints: HashSet<u16>,
    debug_mode: bool,
}

fn parse_addr(s: &str) -> Result<u16, std::num::ParseIntError> {
    if let Some(s) = s.strip_prefix("$") {
        u16::from_str_radix(s, 16)
    } else {
        s.parse()
    }
}

impl Emulator {
    pub fn new() -> Self {
        Emulator {
            cpu: Cpu::new(),
            ppu: Ppu::new(),
            ram: Ram::new(),
            timer: Timer::new(),
            breakpoints: HashSet::new(),
            debug_mode: false,
        }
    }
    pub fn with_debug_mode(dm: bool) -> Self {
        let mut emu = Self::new();
        emu.debug_mode = dm;
        emu
    }
    pub fn debug(&mut self) {
        self.debug_mode = true;
        println!(
            "OP at {:04x}: ${:02x}",
            self.cpu.pc,
            self.ram.read(self.cpu.pc)
        );
        loop {
            let mut input = String::new();
            stdin().read_line(&mut input).unwrap();
            let mut input = input.split_whitespace();
            if let Some(cmd) = input.next() {
                match cmd {
                    "b" => {
                        if let Some(addr) = input.next().and_then(|s| parse_addr(s).ok()) {
                            self.breakpoints.insert(addr);
                            println!("Breakpoint inserted at ${:04x}", addr);
                        }
                    }
                    "c" => {
                        self.debug_mode = false;
                        break;
                    }
                    "d" => {
                        self.breakpoints.clear();
                    }
                    "r" => {
                        self.cpu.print_regs();
                    }
                    "q" => exit(0),
                    "x" => {
                        let Some(s) = input.next() else {
                            continue;
                        };
                        let addr = match s {
                            // "a" => ...
                            "sp" => self.cpu.sp,
                            _ => match parse_addr(s).ok() {
                                Some(addr) => addr,
                                None => continue,
                            },
                        };
                        print!("{:04x}:", addr);
                        for i in 0..16 {
                            print!(" {:02x}", self.ram.read(addr.wrapping_add(i)));
                        }
                        println!();
                        let addr = addr.wrapping_add(16);
                        print!("{:04x}:", addr);
                        for i in 0..16 {
                            print!(" {:02x}", self.ram.read(addr.wrapping_add(i)));
                        }
                        println!();
                    }
                    _ => continue,
                }
            } else {
                break;
            }
        }
    }
    pub fn tick(&mut self) -> u8 {
        if self.debug_mode || self.breakpoints.contains(&self.cpu.pc) {
            self.debug();
        }
        // if !self.cpu.halted {
        //     self.cpu.log(&self.ram);
        // }
        let m_cyc = self.cpu.tick(&mut self.ram);
        let t_cyc = 4 * m_cyc;
        let mut div = self.ram.read(DIV);
        let mut tima = self.ram.read(TIMA);
        let mut if_ = self.ram.read(IF);
        self.timer.tick(
            &mut div,
            &mut tima,
            self.ram.read(TMA),
            self.ram.read(TAC),
            &mut if_,
            t_cyc,
        );
        self.ram.write(DIV, div);
        self.ram.write(TIMA, tima);
        self.ram.write(IF, if_);
        self.ppu.tick(&mut self.ram, t_cyc);
        if self.ram.read(SC) & (1 << 7) > 0 {
            print!("{}", self.ram.read(SB) as char);
            stdout().flush().unwrap();
            self.ram.write(SC, self.ram.read(SC) ^ (1 << 7));
        }
        t_cyc
    }
    pub fn frame_ready(&self) -> bool {
        self.ppu.mode == Mode::Mode1 && self.ram.read(LY) == 153
    }
    pub fn framebuffer(&self) -> &[u8; SCRN_X * SCRN_Y] {
        &self.ppu.fetcher.framebuffer
    }
    pub fn load<R: Read>(&mut self, input: &mut R) -> io::Result<()> {
        self.ram.load(input)
    }
}

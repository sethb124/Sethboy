extern crate sdl2;

use sdl2::event::Event;
use sdl2::keyboard::Keycode;
use std::{
    env::args,
    fs::File,
    process::ExitCode,
    time::{Duration, Instant},
};

use crate::{display::*, emulator::*};

mod display;
mod emulator;

#[allow(unused_variables)]
fn main() -> ExitCode {
    let mut debug = false;
    let mut fname = None;
    let exec_name = args().next().unwrap();
    for arg in args().skip(1) {
        match arg.as_str() {
            "-d" | "--debug" => debug = true,
            _ if fname.is_none() => fname = Some(arg),
            _ => {
                // eprintln!("Unknown option: '{arg}'");
                eprintln!("Usage: {exec_name} [OPTIONS] <file>");
                return ExitCode::FAILURE;
            }
        }
    }
    let Some(fname) = fname else {
        eprintln!("Usage: {exec_name} [OPTIONS] <file>");
        return ExitCode::FAILURE;
    };
    let Ok(mut program) = File::open(&fname) else {
        eprintln!("Unable to open file: {fname}");
        return ExitCode::FAILURE;
    };
    let mut emu = Emulator::with_debug_mode(debug);
    if emu.load(&mut program).is_err() {
        eprintln!("Unable to read file: {fname}");
        return ExitCode::FAILURE;
    }
    let mut disp = Display::new();
    disp.show();
    const CYCLE_DUR: Duration = Duration::from_nanos(238);
    'running: loop {
        let now = Instant::now();
        for event in disp.events() {
            match event {
                Event::Quit { .. }
                | Event::KeyDown {
                    keycode: Some(Keycode::Escape),
                    ..
                } => break 'running,
                _ => {}
            }
        }
        let t_cyc = emu.tick();
        let elapsed = now.elapsed();
        // println!("{:?}", elapsed);
        let expected_time = t_cyc as u32 * CYCLE_DUR;
        if elapsed < expected_time {
            std::thread::sleep(expected_time - elapsed);
        }
        // present frame if ready
        if emu.frame_ready() {
            disp.update(emu.framebuffer());
            // std::thread::sleep(Duration::from_secs(2));
            // break;
        }
    }
    ExitCode::SUCCESS
}

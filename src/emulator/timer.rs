pub struct Timer {
    counter: u16,
}

impl Timer {
    pub fn new() -> Self {
        // TODO: set this to 1024 to simulate boot rom cycles
        Timer { counter: 0 }
    }
    pub fn tick(&mut self, div: &mut u8, tima: &mut u8, tma: u8, tac: u8, if_: &mut u8, t_cyc: u8) {
        // tima increment enabled
        if tac & 0b100 > 0 {
            let mask = 1
                << match tac & 0b11 {
                    0 => 10,
                    1 => 4,
                    2 => 6,
                    3 => 8,
                    _ => unreachable!(),
                };
            for _ in 0..t_cyc {
                let inc = self.counter.wrapping_add(1);
                // bit flipped
                if (inc ^ self.counter) & mask > 0 {
                    let (sum, over) = tima.overflowing_add(1);
                    if over {
                        *tima = tma;
                        *if_ |= 1 << 2;
                    } else {
                        *tima = sum;
                    }
                }
                self.counter = inc;
            }
        } else {
            self.counter = self.counter.wrapping_add(t_cyc as u16);
        }
        *div = (self.counter >> 8) as u8;
    }
}

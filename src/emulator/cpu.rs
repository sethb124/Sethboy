use super::{constants::*, ram::CpuBus};

#[derive(PartialEq, Eq)]
enum Ime {
    Disabled,
    Pending,
    Enabled,
}

#[derive(Clone, Copy)]
struct Flag {
    zero: bool,
    sub: bool,
    half_carry: bool,
    carry: bool,
}

impl From<u8> for Flag {
    fn from(val: u8) -> Self {
        Flag {
            zero: val & (1 << 7) > 0,
            sub: val & (1 << 6) > 0,
            half_carry: val & (1 << 5) > 0,
            carry: val & (1 << 4) > 0,
        }
    }
}

impl From<Flag> for u8 {
    fn from(f: Flag) -> Self {
        (if f.zero { 1 << 7 } else { 0 }
            | if f.sub { 1 << 6 } else { 0 }
            | if f.half_carry { 1 << 5 } else { 0 }
            | if f.carry { 1 << 4 } else { 0 })
    }
}

pub(super) struct Cpu {
    pub(super) pc: u16,
    pub(super) sp: u16,
    a: u8,
    b: u8,
    c: u8,
    d: u8,
    e: u8,
    f: Flag,
    h: u8,
    l: u8,
    // interrupt master enabled
    ime: Ime,
    halted: bool,
    stopped: bool,
}

impl Cpu {
    pub(super) fn new() -> Self {
        Cpu {
            pc: 0x100,
            // sp usually placed here by the boot rom
            sp: 0xFFFE,
            a: 1,
            b: 0,
            c: 0x13,
            d: 0,
            e: 0xD8,
            f: Flag {
                zero: true,
                sub: false,
                half_carry: true,
                carry: true,
            },
            h: 1,
            l: 0x4D,
            ime: Ime::Disabled,
            halted: false,
            stopped: false,
        }
    }
    #[allow(clippy::needless_return)]
    pub(super) fn tick<T: CpuBus>(&mut self, ram: &mut T) -> u8 {
        if self.stopped {
            return 1;
        }
        let if_ = ram.read(IF);
        let ie = ram.read(IE);
        if self.halted {
            if if_ & ie & 0b11111 > 0 {
                self.halted = false;
            } else {
                return 1;
            }
        }
        // self.log(ram);
        if self.ime == Ime::Enabled && if_ & ie & 0b11111 > 0 {
            let bit = (if_ & ie).trailing_zeros() as u16;
            self.push16(ram, self.pc);
            self.pc = 0x40 + bit * 8;
            ram.write(IF, if_ & !(1 << bit));
            self.ime = Ime::Disabled;
            return 5;
        }
        if self.ime == Ime::Pending {
            self.ime = Ime::Enabled;
        }
        let op = self.fetch(ram);
        match (op >> 6) & 0b11 {
            // block 0
            0 => match op & 0b111_111 {
                // nop
                0 => return 1,
                // ld [n16], sp
                0b001_000 => {
                    let n16 = self.fetch16(ram);
                    ram.write(n16, self.sp as u8);
                    ram.write(n16 + 1, (self.sp >> 8) as u8);
                    return 5;
                }
                // rlca
                0b000_111 => {
                    self.a = self.a.rotate_left(1);
                    self.f.zero = false;
                    self.f.sub = false;
                    self.f.half_carry = false;
                    self.f.carry = self.a & 1 > 0;
                    return 1;
                }
                // rrca
                0b001_111 => {
                    self.f.carry = self.a & 1 > 0;
                    self.a = self.a.rotate_right(1);
                    self.f.zero = false;
                    self.f.sub = false;
                    self.f.half_carry = false;
                    return 1;
                }
                // rla
                0b010_111 => {
                    let carry = self.a & (1 << 7) > 0;
                    self.a <<= 1;
                    self.a |= self.f.carry as u8;
                    self.f.zero = false;
                    self.f.sub = false;
                    self.f.half_carry = false;
                    self.f.carry = carry;
                    return 1;
                }
                // rra
                0b011_111 => {
                    let carry = self.a & 1 > 0;
                    self.a >>= 1;
                    self.a |= (self.f.carry as u8) << 7;
                    self.f.zero = false;
                    self.f.sub = false;
                    self.f.half_carry = false;
                    self.f.carry = carry;
                    return 1;
                }
                // daa
                0b100_111 => {
                    let mut adj = 0;
                    if self.f.sub {
                        if self.f.half_carry {
                            adj = 6;
                        }
                        if self.f.carry {
                            adj += 0x60;
                        }
                        self.a = self.a.wrapping_sub(adj);
                    } else {
                        if self.f.half_carry || self.a & 0xF > 9 {
                            adj = 6;
                        }
                        if self.f.carry || self.a > 0x99 {
                            adj += 0x60;
                            self.f.carry = true;
                        }
                        self.a = self.a.wrapping_add(adj);
                    }
                    self.f.zero = self.a == 0;
                    self.f.half_carry = false;
                    return 1;
                }
                // cpl
                0b101_111 => {
                    self.a = !self.a;
                    self.f.sub = true;
                    self.f.half_carry = true;
                    return 1;
                }
                // scf
                0b110_111 => {
                    self.f.sub = false;
                    self.f.half_carry = false;
                    self.f.carry = true;
                    return 1;
                }
                // ccf
                0b111_111 => {
                    self.f.sub = false;
                    self.f.half_carry = false;
                    self.f.carry = !self.f.carry;
                    return 1;
                }
                // jr n8
                0b011_000 => {
                    let offset = self.fetch(ram) as i8;
                    self.pc = (self.pc as i16).wrapping_add(offset as i16) as u16;
                    return 3;
                }
                // stop
                0b010_000 => {
                    // self.stopped = true;
                    return 1;
                }
                _ => match op & 0b1111 {
                    // jr cond, n8
                    0b0000 | 0b1000 => {
                        if self.cond((op >> 3) & 0b11) {
                            let offset = self.fetch(ram) as i8;
                            self.pc = (self.pc as i16).wrapping_add(offset as i16) as u16;
                            return 3;
                        } else {
                            // still need to increment if cond failed
                            self.pc += 1;
                            return 2;
                        }
                    }
                    // ld r16, n16
                    0b0001 => {
                        let n16 = self.fetch16(ram);
                        self.set_r16((op >> 4) & 0b11, n16);
                        return 3;
                    }
                    // ld [r16mem], a
                    0b0010 => {
                        let r = (op >> 4) & 0b11;
                        let r16 = self.get_r16(if r == 3 { 2 } else { r });
                        ram.write(r16, self.a);
                        if r == 2 {
                            // hl+
                            self.set_r16(2, r16.wrapping_add(1));
                        } else if r == 3 {
                            // hl-
                            self.set_r16(2, r16.wrapping_sub(1));
                        }
                        return 2;
                    }
                    // ld a, [r16mem]
                    0b1010 => {
                        let r = (op >> 4) & 0b11;
                        let r16 = self.get_r16(if r == 3 { 2 } else { r });
                        self.a = ram.read(r16);
                        if r == 2 {
                            // hl+
                            self.set_r16(2, r16.wrapping_add(1));
                        } else if r == 3 {
                            // hl-
                            self.set_r16(2, r16.wrapping_sub(1));
                        }
                        return 2;
                    }
                    // inc r16
                    0b0011 => {
                        let r = (op >> 4) & 0b11;
                        self.set_r16(r, self.get_r16(r).wrapping_add(1));
                        return 2;
                    }
                    // dec r16
                    0b1011 => {
                        let r = (op >> 4) & 0b11;
                        self.set_r16(r, self.get_r16(r).wrapping_sub(1));
                        return 2;
                    }
                    // add hl, r16
                    0b1001 => {
                        let r = (op >> 4) & 0b11;
                        let r16 = self.get_r16(r);
                        let hl = self.get_r16(2);
                        let (sum, over) = hl.overflowing_add(r16);
                        self.set_r16(2, sum);
                        self.f.sub = false;
                        self.f.half_carry = (hl & 0xFFF) + (r16 & 0xFFF) > 0xFFF;
                        self.f.carry = over;
                        return 2;
                    }
                    // inc r8
                    0b0100 | 0b1100 => {
                        let r = (op >> 3) & 0b111;
                        let (r8, cyc) = if r == 6 {
                            (&mut ram.read(self.get_r16(2)), 3)
                        } else {
                            (self.get_r8(r), 1)
                        };
                        let over = *r8 & 0xF == 0xF;
                        let sum = r8.wrapping_add(1);
                        *r8 = sum;
                        self.f.zero = sum == 0;
                        self.f.sub = false;
                        self.f.half_carry = over;
                        if r == 6 {
                            ram.write(self.get_r16(2), sum);
                        }
                        return cyc;
                    }
                    // dec r8
                    0b0101 | 0b1101 => {
                        let r = (op >> 3) & 0b111;
                        let (r8, cyc) = if r == 6 {
                            (&mut ram.read(self.get_r16(2)), 3)
                        } else {
                            (self.get_r8(r), 1)
                        };
                        let over = *r8 & 0xF == 0;
                        let sum = r8.wrapping_sub(1);
                        *r8 = sum;
                        self.f.zero = sum == 0;
                        self.f.sub = true;
                        self.f.half_carry = over;
                        if r == 6 {
                            ram.write(self.get_r16(2), sum);
                        }
                        return cyc;
                    }
                    // ld r8, n8
                    0b0110 | 0b1110 => {
                        let n8 = self.fetch(ram);
                        let r = (op >> 3) & 0b111;
                        if r == 6 {
                            ram.write(self.get_r16(2), n8);
                            return 3;
                        }
                        *self.get_r8(r) = n8;
                        return 2;
                    }
                    _ => panic!("{:#010b} is not an instruction", op),
                },
            },
            // block 1
            1 => {
                // halt
                if op == 0b0111_0110 {
                    self.halted = true;
                    if self.ime == Ime::Disabled && if_ & ie & 0b11111 > 0 {
                        // TODO: do halt bug
                        println!("WARNING: HALT BUG NOT IMPLEMENTED!");
                    }
                    return 1;
                // ld r8, r8
                } else {
                    let dest = (op >> 3) & 0b111;
                    let source = op & 0b111;
                    if dest == 6 {
                        ram.write(self.get_r16(2), *self.get_r8(source));
                        return 2;
                    } else if source == 6 {
                        *self.get_r8(dest) = ram.read(self.get_r16(2));
                        return 2;
                    }
                    *self.get_r8(dest) = *self.get_r8(source);
                    return 1;
                }
            }
            // block 2
            2 => match (op >> 3) & 0b111 {
                // add a, r8
                0b000 => {
                    let r = op & 0b111;
                    let (r8, cyc) = if r == 6 {
                        (ram.read(self.get_r16(2)), 2)
                    } else {
                        (*self.get_r8(op & 0b111), 1)
                    };
                    let (sum, over) = self.a.overflowing_add(r8);
                    let halfover = (r8 & 0xF) + (self.a & 0xF) > 0xF;
                    self.a = sum;
                    self.f.zero = sum == 0;
                    self.f.sub = false;
                    self.f.half_carry = halfover;
                    self.f.carry = over;
                    return cyc;
                }
                // adc a, r8
                0b001 => {
                    let r = op & 0b111;
                    let (r8, cyc) = if r == 6 {
                        (ram.read(self.get_r16(2)), 2)
                    } else {
                        (*self.get_r8(r), 1)
                    };
                    let carry = self.f.carry as u8;
                    let (sum1, over1) = self.a.overflowing_add(r8);
                    let (sum2, over2) = sum1.overflowing_add(carry);
                    let halfover = (self.a & 0xF) + (r8 & 0xF) + carry > 0xF;
                    self.a = sum2;
                    self.f.zero = sum2 == 0;
                    self.f.sub = false;
                    self.f.half_carry = halfover;
                    self.f.carry = over1 || over2;
                    return cyc;
                }
                // sub a, r8
                0b010 => {
                    let r = op & 0b111;
                    let (r8, cyc) = if r == 6 {
                        (ram.read(self.get_r16(2)), 2)
                    } else {
                        (*self.get_r8(op & 0b111), 1)
                    };
                    let (sum, over) = self.a.overflowing_sub(r8);
                    let halfover = self.a & 0xF < r8 & 0xF;
                    self.a = sum;
                    self.f.zero = sum == 0;
                    self.f.sub = true;
                    self.f.half_carry = halfover;
                    self.f.carry = over;
                    return cyc;
                }
                // sbc a, r8
                0b011 => {
                    let r = op & 0b111;
                    let (r8, cyc) = if r == 6 {
                        (ram.read(self.get_r16(2)), 2)
                    } else {
                        (*self.get_r8(op & 0b111), 1)
                    };
                    let carry = self.f.carry as u8;
                    let (sum1, over1) = self.a.overflowing_sub(r8);
                    let (sum2, over2) = sum1.overflowing_sub(carry);
                    let halfover = self.a & 0xF < (r8 & 0xF) + carry;
                    self.a = sum2;
                    self.f.zero = sum2 == 0;
                    self.f.sub = true;
                    self.f.half_carry = halfover;
                    self.f.carry = over1 || over2;
                    return cyc;
                }
                // and a, r8
                0b100 => {
                    let r = op & 0b111;
                    let (r8, cyc) = if r == 6 {
                        (ram.read(self.get_r16(2)), 2)
                    } else {
                        (*self.get_r8(op & 0b111), 1)
                    };
                    self.a &= r8;
                    self.f.zero = self.a == 0;
                    self.f.sub = false;
                    self.f.half_carry = true;
                    self.f.carry = false;
                    return cyc;
                }
                // xor a, r8
                0b101 => {
                    let r = op & 0b111;
                    let (r8, cyc) = if r == 6 {
                        (ram.read(self.get_r16(2)), 2)
                    } else {
                        (*self.get_r8(op & 0b111), 1)
                    };
                    self.a ^= r8;
                    self.f.zero = self.a == 0;
                    self.f.sub = false;
                    self.f.half_carry = false;
                    self.f.carry = false;
                    return cyc;
                }
                // or a, r8
                0b110 => {
                    let r = op & 0b111;
                    let (r8, cyc) = if r == 6 {
                        (ram.read(self.get_r16(2)), 2)
                    } else {
                        (*self.get_r8(op & 0b111), 1)
                    };
                    self.a |= r8;
                    self.f.zero = self.a == 0;
                    self.f.sub = false;
                    self.f.half_carry = false;
                    self.f.carry = false;
                    return cyc;
                }
                // cp a, r8
                0b111 => {
                    let r = op & 0b111;
                    let (r8, cyc) = if r == 6 {
                        (ram.read(self.get_r16(2)), 2)
                    } else {
                        (*self.get_r8(op & 0b111), 1)
                    };
                    let (sum, over) = self.a.overflowing_sub(r8);
                    let halfover = self.a & 0xF < r8 & 0xF;
                    self.f.zero = sum == 0;
                    self.f.sub = true;
                    self.f.half_carry = halfover;
                    self.f.carry = over;
                    return cyc;
                }
                _ => panic!("{:#010b} is not an instruction", op),
            },
            // block 3
            3 => match op & 0b111_111 {
                // prefix
                0b001011 => {
                    let op = self.fetch(ram);
                    match (op >> 6) & 0b11 {
                        // bit b3, r8
                        1 => {
                            let r = op & 0b111;
                            let bit = (op >> 3) & 0b111;
                            let (r8, cyc) = if r == 6 {
                                (ram.read(self.get_r16(2)), 3)
                            } else {
                                (*self.get_r8(r), 2)
                            };
                            self.f.zero = r8 & (1 << bit) == 0;
                            self.f.sub = false;
                            self.f.half_carry = true;
                            return cyc;
                        }
                        // res b3, r8
                        2 => {
                            let r = op & 0b111;
                            let bit = (op >> 3) & 0b111;
                            let mask = 1 << bit;
                            if r == 6 {
                                let hl = self.get_r16(2);
                                ram.write(hl, ram.read(hl) & !mask);
                                return 4;
                            };
                            *self.get_r8(op & 0b111) &= !mask;
                            return 2;
                        }
                        // set b3, r8
                        3 => {
                            let r = op & 0b111;
                            let bit = (op >> 3) & 0b111;
                            let mask = 1 << bit;
                            if r == 6 {
                                let hl = self.get_r16(2);
                                ram.write(hl, ram.read(hl) | mask);
                                return 4;
                            };
                            *self.get_r8(op & 0b111) |= mask;
                            return 2;
                        }
                        _ => match (op >> 3) & 0b111 {
                            // rlc r8
                            0b000 => {
                                self.f.sub = false;
                                self.f.half_carry = false;
                                let r = op & 0b111;
                                if r == 6 {
                                    let byte = ram.read(self.get_r16(2));
                                    let rot = byte.rotate_left(1);
                                    self.f.carry = rot & 1 > 0;
                                    self.f.zero = rot == 0;
                                    ram.write(self.get_r16(2), rot);
                                    return 4;
                                }
                                let r8 = self.get_r8(r);
                                let rot = r8.rotate_left(1);
                                *r8 = rot;
                                self.f.carry = rot & 1 > 0;
                                self.f.zero = rot == 0;
                                return 2;
                            }
                            // rrc r8
                            0b001 => {
                                self.f.sub = false;
                                self.f.half_carry = false;
                                let r = op & 0b111;
                                if r == 6 {
                                    let byte = ram.read(self.get_r16(2));
                                    let rot = byte.rotate_right(1);
                                    self.f.carry = byte & 1 > 0;
                                    self.f.zero = rot == 0;
                                    ram.write(self.get_r16(2), rot);
                                    return 4;
                                }
                                let r8 = self.get_r8(r);
                                let temp = *r8;
                                *r8 = r8.rotate_right(1);
                                self.f.zero = *r8 == 0;
                                self.f.carry = temp & 1 > 0;
                                return 2;
                            }
                            // rl r8
                            0b010 => {
                                self.f.sub = false;
                                self.f.half_carry = false;
                                let r = op & 0b111;
                                if r == 6 {
                                    let mut byte = ram.read(self.get_r16(2));
                                    let carry = byte & (1 << 7) > 0;
                                    byte <<= 1;
                                    byte |= self.f.carry as u8;
                                    self.f.carry = carry;
                                    self.f.zero = byte == 0;
                                    ram.write(self.get_r16(2), byte);
                                    return 4;
                                }
                                let old_carry = self.f.carry as u8;
                                let r8 = self.get_r8(r);
                                let new_carry = *r8 & (1 << 7) > 0;
                                *r8 <<= 1;
                                *r8 |= old_carry;
                                self.f.zero = *r8 == 0;
                                self.f.carry = new_carry;
                                return 2;
                            }
                            // rr r8
                            0b011 => {
                                self.f.sub = false;
                                self.f.half_carry = false;
                                let r = op & 0b111;
                                if r == 6 {
                                    let mut byte = ram.read(self.get_r16(2));
                                    let carry = byte & 1 > 0;
                                    byte >>= 1;
                                    byte |= (self.f.carry as u8) << 7;
                                    self.f.carry = carry;
                                    self.f.zero = byte == 0;
                                    ram.write(self.get_r16(2), byte);
                                    return 4;
                                }
                                let old_carry = self.f.carry as u8;
                                let r8 = self.get_r8(r);
                                let new_carry = *r8 & 1 > 0;
                                *r8 >>= 1;
                                *r8 |= old_carry << 7;
                                self.f.zero = *r8 == 0;
                                self.f.carry = new_carry;
                                return 2;
                            }
                            // sla r8
                            0b100 => {
                                self.f.sub = false;
                                self.f.half_carry = false;
                                let r = op & 0b111;
                                if r == 6 {
                                    let mut byte = ram.read(self.get_r16(2));
                                    self.f.carry = byte & (1 << 7) > 0;
                                    byte <<= 1;
                                    self.f.zero = byte == 0;
                                    ram.write(self.get_r16(2), byte);
                                    return 4;
                                }
                                let r8 = self.get_r8(r);
                                let temp = *r8;
                                *r8 <<= 1;
                                self.f.zero = *r8 == 0;
                                self.f.carry = temp & (1 << 7) > 0;
                                return 2;
                            }
                            // sra r8
                            0b101 => {
                                self.f.sub = false;
                                self.f.half_carry = false;
                                let r = op & 0b111;
                                if r == 6 {
                                    let mut byte = ram.read(self.get_r16(2));
                                    let temp = byte;
                                    self.f.carry = byte & 1 > 0;
                                    byte >>= 1;
                                    byte |= temp & (1 << 7);
                                    self.f.zero = byte == 0;
                                    ram.write(self.get_r16(2), byte);
                                    return 4;
                                }
                                let r8 = self.get_r8(r);
                                let temp = *r8;
                                *r8 >>= 1;
                                *r8 |= temp & (1 << 7);
                                self.f.zero = *r8 == 0;
                                self.f.carry = temp & 1 > 0;
                                return 2;
                            }
                            // swap r8
                            0b110 => {
                                self.f.sub = false;
                                self.f.half_carry = false;
                                self.f.carry = false;
                                let r = op & 0b111;
                                if r == 6 {
                                    let mut byte = ram.read(self.get_r16(2));
                                    byte = byte.rotate_right(4);
                                    self.f.zero = byte == 0;
                                    ram.write(self.get_r16(2), byte);
                                    return 4;
                                }
                                let r8 = self.get_r8(r);
                                *r8 = r8.rotate_right(4);
                                self.f.zero = *r8 == 0;
                                return 2;
                            }
                            // srl r8
                            0b111 => {
                                self.f.sub = false;
                                self.f.half_carry = false;
                                let r = op & 0b111;
                                if r == 6 {
                                    let mut byte = ram.read(self.get_r16(2));
                                    self.f.carry = byte & 1 > 0;
                                    byte >>= 1;
                                    self.f.zero = byte == 0;
                                    ram.write(self.get_r16(2), byte);
                                    return 4;
                                }
                                let r8 = self.get_r8(r);
                                let temp = *r8;
                                *r8 >>= 1;
                                self.f.zero = *r8 == 0;
                                self.f.carry = temp & 1 > 0;
                                return 2;
                            }
                            _ => unreachable!(),
                        },
                    }
                }
                // add a, n8
                0b000_110 => {
                    let n8 = self.fetch(ram);
                    let (sum, over) = self.a.overflowing_add(n8);
                    let halfover = (n8 & 0xF) + (self.a & 0xF) > 0xF;
                    self.a = sum;
                    self.f.zero = sum == 0;
                    self.f.sub = false;
                    self.f.half_carry = halfover;
                    self.f.carry = over;
                    return 2;
                }
                // adc a, n8
                0b001_110 => {
                    let n8 = self.fetch(ram);
                    let carry = self.f.carry as u8;
                    let (sum1, over1) = self.a.overflowing_add(n8);
                    let (sum2, over2) = sum1.overflowing_add(carry);
                    let halfover = (self.a & 0xF) + (n8 & 0xF) + carry > 0xF;
                    self.a = sum2;
                    self.f.zero = sum2 == 0;
                    self.f.sub = false;
                    self.f.half_carry = halfover;
                    self.f.carry = over1 || over2;
                    return 2;
                }
                // sub a, n8
                0b010_110 => {
                    let n8 = self.fetch(ram);
                    let (sum, over) = self.a.overflowing_sub(n8);
                    let halfover = self.a & 0xF < n8 & 0xF;
                    self.a = sum;
                    self.f.zero = sum == 0;
                    self.f.sub = true;
                    self.f.half_carry = halfover;
                    self.f.carry = over;
                    return 2;
                }
                // sbc a, n8
                0b011_110 => {
                    let n8 = self.fetch(ram);
                    let carry = self.f.carry as u8;
                    let (sum1, over1) = self.a.overflowing_sub(n8);
                    let (sum2, over2) = sum1.overflowing_sub(carry);
                    let halfover = self.a & 0xF < (n8 & 0xF) + carry;
                    self.a = sum2;
                    self.f.zero = sum2 == 0;
                    self.f.sub = true;
                    self.f.half_carry = halfover;
                    self.f.carry = over1 || over2;
                    return 2;
                }
                // and a, n8
                0b100_110 => {
                    self.a &= self.fetch(ram);
                    self.f.zero = self.a == 0;
                    self.f.sub = false;
                    self.f.half_carry = true;
                    self.f.carry = false;
                    return 2;
                }
                // xor a, n8
                0b101110 => {
                    self.a ^= self.fetch(ram);
                    self.f.zero = self.a == 0;
                    self.f.sub = false;
                    self.f.half_carry = false;
                    self.f.carry = false;
                    return 2;
                }
                // or a, n8
                0b110110 => {
                    self.a |= self.fetch(ram);
                    self.f.zero = self.a == 0;
                    self.f.sub = false;
                    self.f.half_carry = false;
                    self.f.carry = false;
                    return 2;
                }
                // cp a, n8
                0b111_110 => {
                    let n8 = self.fetch(ram);
                    let (sum, over) = self.a.overflowing_sub(n8);
                    let halfover = self.a & 0xF < n8 & 0xF;
                    self.f.zero = sum == 0;
                    self.f.sub = true;
                    self.f.half_carry = halfover;
                    self.f.carry = over;
                    return 2;
                }
                // ret
                0b001_001 => {
                    self.pc = self.pop16(ram);
                    return 4;
                }
                // reti
                0b011_001 => {
                    self.pc = self.pop16(ram);
                    self.ime = Ime::Enabled;
                    return 4;
                }
                // call n16
                0b001_101 => {
                    let n16 = self.fetch16(ram);
                    self.push16(ram, self.pc);
                    self.pc = n16;
                    return 6;
                }
                // jp n16
                0b000_011 => {
                    let n16 = self.fetch16(ram);
                    self.pc = n16;
                    return 4;
                }
                // jp hl
                0b101_001 => {
                    self.pc = self.get_r16(2);
                    return 1;
                }
                // ldh [c], a
                0b100_010 => {
                    ram.write(0xFF00 | self.c as u16, self.a);
                    return 2;
                }
                // ldh [n8], a
                0b100_000 => {
                    let n8 = self.fetch(ram);
                    ram.write(0xFF00 | n8 as u16, self.a);
                    return 3;
                }
                // ld [n16], a
                0b101_010 => {
                    ram.write(self.fetch16(ram), self.a);
                    return 4;
                }
                // ldh a, [c]
                0b110_010 => {
                    self.a = ram.read(0xFF00 | self.c as u16);
                    return 2;
                }
                // ldh a, [n8]
                0b110_000 => {
                    let n8 = self.fetch(ram);
                    self.a = ram.read(0xFF00 | n8 as u16);
                    return 3;
                }
                // ld a, [n16]
                0b111_010 => {
                    self.a = ram.read(self.fetch16(ram));
                    return 4;
                }
                // add sp, n8
                0b101_000 => {
                    let n8 = self.fetch(ram) as i8;
                    self.f.zero = false;
                    self.f.sub = false;
                    self.f.half_carry = ((self.sp & 0xF) + ((n8 as u16) & 0xF)) > 0xF;
                    self.f.carry = ((self.sp & 0xFF) + ((n8 as u16) & 0xFF)) > 0xFF;
                    self.sp = (self.sp as i16).wrapping_add(n8 as i16) as u16;
                    return 4;
                }
                // ld hl, sp + n8
                0b111_000 => {
                    let n8 = self.fetch(ram) as i8;
                    self.f.zero = false;
                    self.f.sub = false;
                    self.f.half_carry = ((self.sp & 0xF) + ((n8 as u16) & 0xF)) > 0xF;
                    self.f.carry = ((self.sp & 0xFF) + ((n8 as u16) & 0xFF)) > 0xFF;
                    self.set_r16(2, (self.sp as i16).wrapping_add(n8 as i16) as u16);
                    return 3;
                }
                // ld sp, hl
                0b111_001 => {
                    self.sp = self.get_r16(2);
                    return 2;
                }
                // di
                0b110_011 => {
                    self.ime = Ime::Disabled;
                    return 1;
                }
                // ei
                0b111_011 => {
                    self.ime = Ime::Pending;
                    return 1;
                }
                _ => match op & 0b1111 {
                    // ret cond
                    0b0000 | 0b1000 => {
                        if self.cond((op >> 3) & 0b11) {
                            self.pc = self.pop16(ram);
                            return 5;
                        } else {
                            return 2;
                        }
                    }
                    // jp cond, n16
                    0b0010 | 0b1010 => {
                        if self.cond((op >> 3) & 0b11) {
                            self.pc = self.fetch16(ram);
                            return 4;
                        } else {
                            // still need to increment if cond failed
                            self.pc += 2;
                            return 3;
                        }
                    }
                    // call cond, n16
                    0b0100 | 0b1100 => {
                        if self.cond((op >> 3) & 0b11) {
                            let n16 = self.fetch16(ram);
                            self.push16(ram, self.pc);
                            self.pc = n16;
                            return 6;
                        } else {
                            // still need to increment if cond failed
                            self.pc += 2;
                            return 3;
                        }
                    }
                    // rst tgt3
                    0b0111 | 0b1111 => {
                        self.push16(ram, self.pc);
                        self.pc = ((op >> 3) & 0b111) as u16 * 8;
                        return 4;
                    }
                    // pop r16stk
                    0b0001 => {
                        let r = (op >> 4) & 0b11;
                        let low = self.pop(ram);
                        let high = self.pop(ram);
                        if r == 3 {
                            self.f = low.into();
                            self.a = high;
                        } else {
                            self.set_r16(r, low as u16 | ((high as u16) << 8));
                        }
                        return 3;
                    }
                    // push r16stk
                    0b0101 => {
                        let r = (op >> 4) & 0b11;
                        if r == 3 {
                            self.push(ram, self.a);
                            self.push(ram, self.f.into());
                        } else {
                            self.push16(ram, self.get_r16(r));
                        }
                        return 4;
                    }
                    _ => {
                        if op == 0xED {
                            self.print_regs();
                        }
                        panic!("{:#04x} is not an instruction", op);
                    }
                },
            },
            // _ => panic!("{:#010b} is not an instruction", op),
            _ => panic!("{:#04x} is not an instruction", op),
        }
    }
    fn fetch<T: CpuBus>(&mut self, ram: &T) -> u8 {
        let val = ram.read(self.pc);
        self.pc += 1;
        val
    }
    fn fetch16<T: CpuBus>(&mut self, ram: &T) -> u16 {
        self.fetch(ram) as u16 | ((self.fetch(ram) as u16) << 8)
    }
    fn pop<T: CpuBus>(&mut self, ram: &T) -> u8 {
        let val = ram.read(self.sp);
        self.sp += 1;
        val
    }
    fn pop16<T: CpuBus>(&mut self, ram: &T) -> u16 {
        self.pop(ram) as u16 | ((self.pop(ram) as u16) << 8)
    }
    fn push<T: CpuBus>(&mut self, ram: &mut T, val: u8) {
        self.sp -= 1;
        ram.write(self.sp, val);
    }
    fn push16<T: CpuBus>(&mut self, ram: &mut T, val: u16) {
        self.push(ram, (val >> 8) as u8);
        self.push(ram, val as u8);
    }
    fn get_r8(&mut self, r: u8) -> &mut u8 {
        match r {
            0 => &mut self.b,
            1 => &mut self.c,
            2 => &mut self.d,
            3 => &mut self.e,
            4 => &mut self.h,
            5 => &mut self.l,
            6 => panic!("[hl] should be handled explicitly"),
            7 => &mut self.a,
            _ => unreachable!(),
        }
    }
    fn get_r16(&self, r: u8) -> u16 {
        let (high, low) = match r {
            0 => (self.b, self.c),
            1 => (self.d, self.e),
            2 => (self.h, self.l),
            3 => return self.sp,
            _ => unreachable!(),
        };
        low as u16 | ((high as u16) << 8)
    }
    fn set_r16(&mut self, r: u8, val: u16) {
        let (high, low) = match r {
            0 => (&mut self.b, &mut self.c),
            1 => (&mut self.d, &mut self.e),
            2 => (&mut self.h, &mut self.l),
            3 => {
                self.sp = val;
                return;
            }
            _ => unreachable!(),
        };
        *high = (val >> 8) as u8;
        *low = val as u8;
    }
    fn cond(&self, c: u8) -> bool {
        match c {
            // nz
            0 => !self.f.zero,
            // z
            1 => self.f.zero,
            // nc
            2 => !self.f.carry,
            // c
            3 => self.f.carry,
            _ => unreachable!(),
        }
    }
    pub fn print_regs(&self) {
        println!(
            "AF: ${:04x}",
            ((self.a as u16) << 8) | u8::from(self.f) as u16
        );
        println!("BC: ${:04x}", self.get_r16(0));
        println!("DE: ${:04x}", self.get_r16(1));
        println!("HL: ${:04x}", self.get_r16(2));
        println!("SP: ${:04x}", self.get_r16(3));
    }
    #[allow(dead_code)]
    pub fn log<T: CpuBus>(&self, ram: &T) {
        // A:00 F:11 B:22 C:33 D:44 E:55 H:66 L:77 SP:8888 PC:9999 PCMEM:AA,BB,CC,DD
        println!(
            "A:{:02X} F:{:02X} B:{:02X} C:{:02X} D:{:02X} E:{:02X} H:{:02X} L:{:02X} SP:{:04X} PC:{:04X} PCMEM:{:02X},{:02X},{:02X},{:02X}",
            self.a,
            u8::from(self.f),
            self.b,
            self.c,
            self.d,
            self.e,
            self.h,
            self.l,
            self.sp,
            self.pc,
            ram.read(self.pc),
            ram.read(self.pc + 1),
            ram.read(self.pc + 2),
            ram.read(self.pc + 3),
        )
    }
}

/*

All graphics are 64x64 points.
First bit of every octet indicates next byte is part of current character.

- Control?
- - 0
- - - 7 bit control code
- - 1
- - - Graphical Data
- - - - bit X
- - - - bit Y
- - - - X? 7 bit signed int horizontal offset in points
- - - - Y? 7 bit signed int vertical offset in points
- - - - Bit Quadtree that caches homogeneity
- - - - - Homogenous?
- - - - - - 0
- - - - - - - Read next level
- - - - - - 1
- - - - - - - Fill quadrant with next bit.

Empty graphical data (all 0s) indicates whitespace. Interpretation left to
renderer. May utilize offsets for extra data or interpretation of whitespace,
eg "x > 0" means space or "y < 0" means newline.
*/

use bitstream_io::{BigEndian, BitRead, BitReader, Endianness};
use std::io::Read;

pub struct BitReaderWrapper<T: Read, E: Endianness> {
    bit_count: usize,
    bitreader: BitReader<T, E>,
}

// Subject to change
pub enum ControlCode {
    DirectionRightDown = 4,
    DirectionLeftDown = 5,
    DirectionRightUp = 6,
    DirectionLeftUp = 7,
}

impl<T: Read, E: Endianness> BitRead for BitReaderWrapper<T, E> {
    fn read_bit(&mut self) -> std::io::Result<bool> {
        if self.bit_count % 8 == 0 {
            self.bitreader.read_bit().ok();
            self.bit_count += 1;
        }
        self.bit_count += 1;
        self.bitreader.read_bit()
    }

    fn read<U>(&mut self, mut bits: u32) -> std::io::Result<U>
    where
        U: bitstream_io::Numeric,
    {
        let mut n = U::default();
        while bits > 0 {
            if self.bit_count % 8 == 0 {
                self.bitreader.read_bit().ok();
                self.bit_count += 1;
            }
            n <<= 1;
            if self.bitreader.read_bit().unwrap() {
                n |= U::ONE;
            }
            self.bit_count += 1;
            bits -= 1;
        }
        Ok(n)
    }

    fn read_signed<S>(&mut self, bits: u32) -> std::io::Result<S>
    where
        S: bitstream_io::SignedNumeric,
    {
        if self.bit_count % 8 == 0 {
            self.bitreader.read_bit().ok();
            self.bit_count += 1;
        }
        let sign = self.bitreader.read_bit().unwrap();
        self.bit_count += 1;
        let n = self.read::<S>(bits - 1).unwrap();
        if sign {
            Ok(n.as_negative(bits))
        } else {
            Ok(n)
        }
    }

    fn read_to<V>(&mut self) -> std::io::Result<V>
    where
        V: bitstream_io::Primitive,
    {
        unimplemented![]
    }

    fn read_as_to<F, V>(&mut self) -> std::io::Result<V>
    where
        F: Endianness,
        V: bitstream_io::Primitive,
    {
        unimplemented![]
    }

    fn skip(&mut self, bits: u32) -> std::io::Result<()> {
        self.bit_count += bits as usize;
        self.bitreader.skip(bits)
    }

    fn byte_aligned(&self) -> bool {
        unimplemented![]
    }

    fn byte_align(&mut self) {
        unimplemented![]
    }
}

pub struct Character {
    pub x_offset: i8,
    pub y_offset: i8,
    pub control_code: Option<u8>,
    pub graphical_data: Option<[u8; 512]>,
}

pub fn decode(input: &[u8]) -> Vec<Character> {
    let mut input = BitReaderWrapper {
        bit_count: 0,
        bitreader: BitReader::<&[u8], BigEndian>::new(input),
    };
    let mut output = vec![];
    while let Ok(graphical) = input.bitreader.read_bit() {
        input.bit_count += 1;
        if graphical {
            let offset_x = input.read_bit().unwrap();
            let offset_y = input.read_bit().unwrap();
            let mut x_offset = 0i8;
            let mut y_offset = 0i8;
            if offset_x {
                x_offset = input.read::<i8>(7).unwrap();
            }
            if offset_y {
                y_offset = input.read_signed::<i8>(7).unwrap();
            }
            let mut map = [0u64; 64];
            let mut value = None;
            let mut erase = -1;
            for (index, shift, q) in (0..4096).map(|n| {
                (
                    n / 64,
                    n % 64,
                    [
                        n / 1024 % 4,
                        n / 256 % 4,
                        n / 64 % 4,
                        n / 16 % 4,
                        n / 4 % 4,
                        n % 4,
                    ],
                )
            }) {
                for i in 0..q.len() {
                    if value.is_none() {
                        if q.ends_with(&vec![0; q.len() - i]) {
                            if input.read_bit().unwrap() {
                                erase = i as i32;
                                value = Some(input.read_bit().unwrap());
                            }
                        }
                    }
                }
                map[index] |= if let Some(v) = value {
                    if v {
                        1 << shift
                    } else {
                        0
                    }
                } else if input.read_bit().unwrap() {
                    1 << shift
                } else {
                    0
                };
                for i in 0..q.len() {
                    if q.ends_with(&vec![0; q.len() - i]) {
                        if erase == i as i32 {
                            value = None;
                            erase = -1;
                        }
                    }
                }
            }
            let mut raster_data: [u8; 512] = [0; 512];
            let mut iter = map.iter().map(|t6| t6.to_le_bytes()).flatten();
            raster_data.fill_with(|| iter.next().unwrap());
            output.push(Character {
                x_offset,
                y_offset,
                control_code: None,
                graphical_data: Some(raster_data),
            });
            input.skip(8 - input.bit_count as u32 % 8).ok();
        } else {
            output.push(Character {
                x_offset: 0,
                y_offset: 0,
                control_code: input.read::<u8>(7).unwrap().into(),
                graphical_data: None,
            })
        }
    }
    output
}

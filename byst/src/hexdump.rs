use std::fmt::{
    Debug,
    Display,
    Write as _,
};

use super::buf::Buf;
use crate::{
    copy_io,
    BufMut,
};

#[inline]
pub fn hexdump<B>(buf: B) -> Hexdump<B> {
    Hexdump::new(buf)
}

pub struct Hexdump<B> {
    buf: B,
    config: Config,
}

impl<B> Hexdump<B> {
    #[inline]
    pub fn new(buf: B) -> Self {
        Self::with_config(buf, Default::default())
    }

    #[inline]
    pub fn with_config(buf: B, config: Config) -> Self {
        Self { buf, config }
    }
}

impl<B: Buf> Display for Hexdump<B> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let mut lines = Lines::new(&self.buf, &self.config);

        if self.config.header {
            writeln!(f, "Hexdump: {} bytes", self.buf.len())?;
        }

        if let Some(line) = lines.next() {
            write!(f, "{line}")?;
        }

        for line in lines {
            write!(f, "\n{line}")?;
        }

        if self.config.trailing_newline {
            writeln!(f)?;
        }

        Ok(())
    }
}

impl<B: Buf> Debug for Hexdump<B> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let hex = Hexdump {
            buf: &self.buf,
            config: Config {
                offset: self.config.offset,
                trailing_newline: false,
                at_least_one_line: false,
                header: false,
            },
        };
        Display::fmt(&hex, f)?;
        Ok(())
    }
}

#[derive(Clone, Copy, Debug)]
pub struct Config {
    pub offset: usize,
    pub trailing_newline: bool,
    pub at_least_one_line: bool,
    pub header: bool,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            offset: 0,
            trailing_newline: true,
            at_least_one_line: true,
            header: true,
        }
    }
}

pub struct Lines<'b, B: Buf + 'b> {
    reader: B::Reader<'b>,
    pad_offset_to: usize,
    offset: usize,
    remaining: usize,
    emit_empty_line: bool,
}

impl<'b, B: Buf> Lines<'b, B> {
    pub fn new(buf: &'b B, config: &Config) -> Self {
        let pad_offset_to = std::cmp::max(num_hex_digits(config.offset + buf.len()), 4);
        Self {
            reader: buf.reader(),
            pad_offset_to,
            offset: config.offset,
            remaining: buf.len(),
            emit_empty_line: config.at_least_one_line,
        }
    }
}

impl<'b, B: Buf> Iterator for Lines<'b, B> {
    type Item = Line;

    fn next(&mut self) -> Option<Self::Item> {
        (self.remaining > 0 || self.emit_empty_line).then(|| {
            self.emit_empty_line = false;

            let mut line = [0; 16];
            let num_bytes = copy_io(line.writer(), &mut self.reader, 16);

            let offset = self.offset;
            self.offset += num_bytes;
            self.remaining -= num_bytes;

            Line {
                line,
                num_bytes,
                offset,
                pad_offset_to: self.pad_offset_to,
            }
        })
    }
}

pub struct Line {
    pub line: [u8; 16],
    pub num_bytes: usize,
    pub offset: usize,
    pub pad_offset_to: usize,
}

impl Display for Line {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        // print offset
        for _ in 0..(self.pad_offset_to - num_hex_digits(self.offset)) {
            write!(f, "0")?;
        }
        write!(f, "{:x} ", self.offset)?;

        if !self.line.is_empty() {
            // print bytes
            for b in &self.line[0..self.num_bytes] {
                write!(f, " {b:02x}")?;
            }

            // pad bytes
            for _ in self.num_bytes..16 {
                write!(f, "   ")?;
            }
            write!(f, "  ")?;

            // print chars
            for b in &self.line[0..self.num_bytes] {
                if b.is_ascii() && !b.is_ascii_control() {
                    f.write_char((*b).into())?;
                }
                else {
                    write!(f, ".")?;
                }
            }
        }

        Ok(())
    }
}

fn num_hex_digits(mut num: usize) -> usize {
    if num == 0 {
        1
    }
    else {
        let mut d = 0usize;
        while num != 0 {
            d += 1;
            num >>= 4;
        }
        d
    }
}

#[cfg(test)]
mod tests {
    use super::Hexdump;

    #[test]
    fn test_display() {
        let data = b"Lorem ipsum dolor sit amet, consectetur adipiscing elit, sed do eiusmod tempor incididunt ut labore et dolore magna aliqua.";
        let formatted = Hexdump::new(&data).to_string();
        let expected = r#"Hexdump: 123 bytes
0000  4c 6f 72 65 6d 20 69 70 73 75 6d 20 64 6f 6c 6f  Lorem ipsum dolo
0010  72 20 73 69 74 20 61 6d 65 74 2c 20 63 6f 6e 73  r sit amet, cons
0020  65 63 74 65 74 75 72 20 61 64 69 70 69 73 63 69  ectetur adipisci
0030  6e 67 20 65 6c 69 74 2c 20 73 65 64 20 64 6f 20  ng elit, sed do 
0040  65 69 75 73 6d 6f 64 20 74 65 6d 70 6f 72 20 69  eiusmod tempor i
0050  6e 63 69 64 69 64 75 6e 74 20 75 74 20 6c 61 62  ncididunt ut lab
0060  6f 72 65 20 65 74 20 64 6f 6c 6f 72 65 20 6d 61  ore et dolore ma
0070  67 6e 61 20 61 6c 69 71 75 61 2e                 gna aliqua.
"#;
        if expected != formatted {
            panic!(
                r#"expected:
{expected}

got:
{formatted}
"#
            );
        }
    }
}

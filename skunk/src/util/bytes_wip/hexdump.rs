use std::fmt::{
    Display,
    Write as _,
};

use super::{
    Buf,
    Cursor,
    Reader,
    Remaining,
};

pub struct HexdumpLines<B> {
    cursor: Cursor<B>,
    pad_offset_to: usize,
    offset: usize,
    emit_empty_line: bool,
}

impl<B: Buf> HexdumpLines<B> {
    pub fn new(buf: B, offset: usize) -> Self {
        let pad_offset_to = num_hex_digits(offset + buf.len());
        Self {
            cursor: Cursor::new(buf),
            pad_offset_to,
            offset,
            emit_empty_line: true,
        }
    }
}

impl<B: Buf> Iterator for HexdumpLines<B> {
    type Item = HexdumpLine;

    fn next(&mut self) -> Option<Self::Item> {
        let remaining = self.cursor.remaining();
        (remaining > 0 || self.emit_empty_line).then(|| {
            self.emit_empty_line = false;
            let num_bytes = std::cmp::min(remaining, 16);
            let mut line = [0; 16];
            self.cursor
                .read_into(&mut line)
                .unwrap_or_else(|_| panic!("Expected at least {num_bytes} more bytes"));
            HexdumpLine {
                line,
                num_bytes,
                offset: self.offset,
                pad_offset_to: self.pad_offset_to,
            }
        })
    }
}

pub struct HexdumpLine {
    pub line: [u8; 16],
    pub num_bytes: usize,
    pub offset: usize,
    pub pad_offset_to: usize,
}

impl Display for HexdumpLine {
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

pub struct Hexdump<B> {
    buf: B,
    offset: usize,
}

impl<B> Hexdump<B> {
    pub fn new(buf: B) -> Self {
        Self { buf, offset: 0 }
    }

    pub fn with_offset(mut self, offset: usize) -> Self {
        self.offset = offset;
        self
    }
}

impl<B: Buf> Display for Hexdump<B> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        for line in HexdumpLines::new(&self.buf, self.offset) {
            writeln!(f, "{line}")?;
        }
        Ok(())
    }
}

fn num_hex_digits(mut num: usize) -> usize {
    let mut d = 0usize;
    while num != 0 {
        d += 1;
        num >>= 4;
    }
    d
}

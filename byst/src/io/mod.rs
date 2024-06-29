mod count;
mod limit;
mod read;
mod write;

pub use byst_macros::{
    Read,
    Write,
};

pub use self::{
    count::Count,
    limit::Limit,
    read::{
        read,
        BufReader,
        End,
        InvalidDiscriminant,
        Read,
        ReadError,
        Reader,
        ReaderExt,
    },
    write::{
        BufWriter,
        Full,
        Write,
        Writer,
        WriterExt,
    },
};

/// A reader or writer that also has knowledge about the position in the
/// underlying buffer.
pub trait Seek {
    type Position;

    fn tell(&self) -> Self::Position;
    fn seek(&mut self, position: &Self::Position) -> Self::Position;
}

impl<'a, T: Seek> Seek for &'a mut T {
    type Position = T::Position;

    #[inline]
    fn tell(&self) -> Self::Position {
        T::tell(*self)
    }

    #[inline]
    fn seek(&mut self, position: &Self::Position) -> Self::Position {
        T::seek(*self, position)
    }
}

impl<'a> Seek for &'a [u8] {
    type Position = &'a [u8];

    #[inline]
    fn tell(&self) -> Self::Position {
        self
    }

    #[inline]
    fn seek(&mut self, position: &Self::Position) -> Self::Position {
        std::mem::replace(self, *position)
    }
}

/// A reader or writer that knows how many bytes are remaining.
pub trait Remaining {
    fn remaining(&self) -> usize;

    #[inline]
    fn is_at_end(&self) -> bool {
        self.remaining() == 0
    }
}

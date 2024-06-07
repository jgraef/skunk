mod array_buf;
mod buf;
mod bytes;
mod endianness;
mod read;

use std::ops::{
    Bound,
    RangeBounds,
};

pub use self::{
    array_buf::ArrayBuf,
    buf::{
        copy,
        Buf,
        BufMut,
        CopyError,
        RangeOutOfBounds,
        SingleChunk,
        SingleChunkMut,
    },
    bytes::{
        Bytes,
        Sbytes,
    },
    endianness::{
        BigEndian,
        Endianness,
        LittleEndian,
        NativeEndian,
        NetworkEndian,
    },
    read::{
        End,
        Read,
        Reader,
    },
};

#[inline]
fn range_bounds_to_slice_index(range: &impl RangeBounds<usize>) -> (Bound<usize>, Bound<usize>) {
    (range.start_bound().cloned(), range.end_bound().cloned())
}

#[inline]
fn slice_get_range<R: RangeBounds<usize>>(
    slice: &[u8],
    range: R,
) -> Result<&[u8], RangeOutOfBounds<R>> {
    slice
        .get(range_bounds_to_slice_index(&range))
        .ok_or_else(|| {
            RangeOutOfBounds {
                range,
                buf_length: slice.len(),
            }
        })
}

#[inline]
fn slice_get_mut_range<R: RangeBounds<usize>>(
    slice: &mut [u8],
    range: R,
) -> Result<&mut [u8], RangeOutOfBounds<R>> {
    let buf_length = slice.len();
    slice
        .get_mut(range_bounds_to_slice_index(&range))
        .ok_or_else(|| RangeOutOfBounds { range, buf_length })
}

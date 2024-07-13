use std::cmp::Ordering;

use crate::{
    buf::{
        Buf,
        BufMut,
        Full,
    },
    io::{
        BufReader,
        BufWriter,
    },
    Range,
    RangeOutOfBounds,
};

/// Copies bytes from `source` to `destination`.
pub fn copy(mut destination: impl BufMut, source: impl Buf) -> Result<(), Full> {
    let source_len = source.len();
    destination.reserve(source_len)?;

    let writer = destination.writer();
    let reader = source.reader();

    let total_copied = copy_io(writer, reader, None);

    match total_copied.cmp(&source_len) {
        Ordering::Equal => {}
        Ordering::Less => {
            panic!("Reserved {source_len} bytes, but only {total_copied} bytes could be written.");
        }
        Ordering::Greater => {
            panic!("Copied buffer with length {source_len}, but {total_copied} bytes were copied.");
        }
    }

    Ok(())
}

/// Error while copying from a [`Buf`] to a [`BufMut`].
#[derive(Debug, PartialEq, Eq, thiserror::Error)]
#[error("Copy error")]
pub enum CopyRangeError {
    #[error("Source index out of bounds")]
    SourceRangeOutOfBounds(RangeOutOfBounds),

    LengthMismatch(#[from] LengthMismatch),

    DestinationFull(#[source] Full),
}

#[derive(Debug, PartialEq, Eq, thiserror::Error)]
#[error("Length mismatch: destination ({destination_length}) != source ({source_length})")]
pub struct LengthMismatch {
    pub destination_range: Range,
    pub destination_length: usize,
    pub source_range: Range,
    pub source_length: usize,
}

/// Copies bytes from `source` to `destination` with respective ranges.
pub fn copy_range(
    mut destination: impl BufMut,
    destination_range: impl Into<Range>,
    source: impl Buf,
    source_range: impl Into<Range>,
) -> Result<(), CopyRangeError> {
    let destination_range: Range = destination_range.into();
    let source_range: Range = source_range.into();

    let destination_start = destination_range.start.unwrap_or_default();
    let source_start = source_range.start.unwrap_or_default();
    let source_end = if let Some(end) = source_range.end {
        if end > source.len() {
            return Err(CopyRangeError::SourceRangeOutOfBounds(RangeOutOfBounds {
                required: source_range,
                bounds: (0, source.len()),
            }));
        }
        end
    }
    else {
        source.len()
    };
    let source_length = source_end - source_start;
    let destination_end = destination_range
        .end
        .unwrap_or_else(|| destination_start + source_length);
    let destination_length = destination_end - destination_start;

    if destination_length != source_length {
        return Err(LengthMismatch {
            destination_range,
            destination_length,
            source_range,
            source_length,
        }
        .into());
    }

    let mut destination_writer = destination.writer();
    if destination_start != 0 {
        destination_writer
            .advance(destination_start)
            .map_err(|e| CopyRangeError::DestinationFull(e.into()))?;
    }

    let mut source_reader = source.reader();
    if source_start != 0 {
        source_reader
            .advance(source_start)
            .expect("Advancing the source reader to {source_start} unexpectedly failed.");
    }

    let total_copied = copy_io(destination_writer, source_reader, source_length);

    assert_eq!(
        total_copied, source_length,
        "Expected to copy {source_length} bytes, but copied {total_copied}."
    );

    Ok(())
}

/// Copies `amount` bytes from `source` to `destination`.
pub fn copy_io(
    mut destination: impl BufWriter,
    mut source: impl BufReader,
    amount: impl Into<Option<usize>>,
) -> usize {
    let mut amount = amount.into();
    let mut total_copied = 0;

    while amount.map_or(true, |n| n > 0) {
        match (destination.peek_chunk_mut(), source.peek_chunk()) {
            (Some(dest_chunk), Some(src_chunk)) => {
                let mut n = std::cmp::min(dest_chunk.len(), src_chunk.len());
                if let Some(amount) = &mut amount {
                    n = std::cmp::min(n, *amount);
                    *amount -= n;
                }

                dest_chunk[..n].copy_from_slice(&src_chunk[..n]);

                total_copied += n;

                destination
                    .advance(n)
                    .expect("Expected at least {n} more bytes in BufWriter");
                source
                    .advance(n)
                    .expect("Expected at least {n} more bytes in BufReader");
            }
            (None, Some(src_chunk)) => {
                if let Err(crate::io::Full { written, .. }) = destination.extend(src_chunk) {
                    // todo: we could try to fill any remaining bytes in the destination.
                    total_copied += written;
                    break;
                }

                total_copied += src_chunk.len();
                source
                    .advance(src_chunk.len())
                    .expect("Expected at least {n} more bytes in BufReader");
            }
            (_, None) => break,
        }
    }

    total_copied
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fails_if_source_longer_than_destination() {
        let mut destination = [0; 4];
        let source = [1; 16];

        match copy(&mut destination, source) {
            Ok(_) => panic!("copy didn't fail"),
            Err(Full {
                required,
                capacity: buf_length,
            }) => {
                assert_eq!(required, 16);
                assert_eq!(buf_length, 4);
            }
        }
    }

    #[test]
    fn it_copies() {
        let mut destination = [42; 16];
        let mut source = [0; 16];
        for i in 0..16 {
            source[i] = i as u8;
        }
        let expected = source;

        match copy(&mut destination, source) {
            Err(e) => panic!("copy failed: {e:?}"),
            Ok(()) => {
                assert_eq!(expected, destination);
            }
        }
    }

    #[test]
    fn it_copies_io() {
        let mut destination: [u8; 8] = [42; 8];
        let source: [u8; 8] = [1, 2, 3, 4, 5, 6, 7, 8];

        let total_copied = copy_io(destination.as_mut(), &mut source.as_ref(), None);
        assert_eq!(total_copied, 8);
        assert_eq!(source, destination);
    }

    #[test]
    fn it_copies_partial() {
        let mut destination: [u8; 8] = [42; 8];
        let source: [u8; 8] = [1, 2, 3, 4, 5, 6, 7, 8];

        let total_copied = copy_io(destination.as_mut(), &mut source.as_ref(), Some(4));
        assert_eq!(total_copied, 4);
        assert_eq!([1, 2, 3, 4, 42, 42, 42, 42], destination);
    }
}

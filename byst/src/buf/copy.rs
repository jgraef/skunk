use super::{
    Buf,
    BufMut,
    Full,
    Range,
    RangeOutOfBounds,
};
use crate::{
    buf::chunks::NonEmptyIter,
    util::Peekable,
};

/// Error while copying from a [`Buf`] to a [`BufMut`].
#[derive(Debug, PartialEq, Eq, thiserror::Error)]
#[error("Copy error")]
pub enum CopyError {
    #[error("Destination index out of bounds")]
    DestinationRangeOutOfBounds(RangeOutOfBounds),

    #[error("Source index out of bounds")]
    SourceRangeOutOfBounds(RangeOutOfBounds),

    LengthMismatch(#[from] LengthMismatch),

    Full(#[from] Full),
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
///
/// This can fail if either range is out of bounds, or the lengths of both
/// ranges aren't equal. See [`CopyError`].
pub fn copy(
    mut destination: impl BufMut,
    destination_range: impl Into<Range>,
    source: impl Buf,
    source_range: impl Into<Range>,
) -> Result<usize, CopyError> {
    let destination_range: Range = destination_range.into();
    let source_range: Range = source_range.into();

    let destination_start = destination_range.start.unwrap_or_default();
    let source_start = source_range.start.unwrap_or_default();
    let source_end = if let Some(end) = source_range.end {
        if end > source.len() {
            return Err(CopyError::SourceRangeOutOfBounds(RangeOutOfBounds {
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
        .unwrap_or_else(|| std::cmp::max(destination.len(), destination_start + source_length));
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

    // till where we can overwrite existing chunks in the destination
    let destination_write_end = std::cmp::min(destination_end, destination.len());
    let destination_write_end =
        (destination_write_end > destination_start).then_some(destination_write_end);

    // up to where we need to grow the destination buffer, filling it with zeros.
    let destination_grow_size =
        (destination_start > destination.len()).then_some(destination_start);

    assert!(!destination_write_end.is_some() || !destination_grow_size.is_some(), "overwriting and growing");

    // reserve space in destination
    // this will also fail early if there is not enough space in the destination
    // buffer.
    destination.reserve(destination_end)?;

    let mut total_copied = 0;

    let mut source_chunks = Peekable::new(NonEmptyIter(
        source
            .chunks(source_range)
            .map_err(CopyError::SourceRangeOutOfBounds)?,
    ));
    let mut source_offset = 0;

    if let Some(destination_write_end) = destination_write_end {
        // overwrite existing part of the destination buffer

        let mut destination_chunks = Peekable::new(NonEmptyIter(
            destination
                .chunks_mut(destination_start..destination_write_end)
                .map_err(CopyError::DestinationRangeOutOfBounds)?,
        ));
        let mut destination_offset = 0;

        total_copied += copy_chunks(
            &mut destination_chunks,
            &mut destination_offset,
            &mut source_chunks,
            &mut source_offset,
            None,
        )?;
    }

    if let Some(destination_grow_size) = destination_grow_size {
        // pad with zeros before extending destination buffer

        destination.grow(destination_grow_size, 0)?;
    }

    // write remainder from source chunks
    while let Some(source_chunk) = source_chunks.next() {
        destination.extend(&source_chunk[source_offset..])?;
        total_copied += source_chunk.len();
        source_offset = 0;
    }

    Ok(total_copied)
}

/// Copies `amount`` bytes from `source_chunks` to `destination_chunks` with the
/// respective starting offsets.
///
/// The chunk iterators must be wrapped with [`Peekable`].
///
/// When this function starts, it will start with the chunks in the peek buffer,
/// and use the specified offsets.
///
/// When this function returns, the chunk that was currently being copied
/// to/from is in the peek buffer. The function returns the offsets in these
/// chunks at which the copy ended.
pub fn copy_chunks<'d, 's, D: Iterator<Item = &'d mut [u8]>, S: Iterator<Item = &'s [u8]>>(
    destination_chunks: &mut Peekable<D>,
    destination_offset: &mut usize,
    source_chunks: &mut Peekable<S>,
    source_offset: &mut usize,
    amount: impl Into<Option<usize>>,
) -> Result<usize, CopyError> {
    let mut amount = amount.into();
    let mut total_copied = 0;

    while amount.map_or(true, |n| n > 0) {
        match (destination_chunks.peek_mut(), source_chunks.peek()) {
            (Some(dest_chunk), Some(src_chunk)) => {
                let mut n = std::cmp::min(
                    dest_chunk.len() - *destination_offset,
                    src_chunk.len() - *source_offset,
                );
                if let Some(amount) = &mut amount {
                    n = std::cmp::min(n, *amount);
                    *amount -= n;
                }

                dest_chunk[*destination_offset..][..n]
                    .copy_from_slice(&src_chunk[*source_offset..][..n]);

                *destination_offset += n;
                *source_offset += n;
                total_copied += n;

                if *destination_offset == dest_chunk.len() {
                    destination_chunks.next();
                    *destination_offset = 0;
                }
                if *source_offset == src_chunk.len() {
                    source_chunks.next();
                    *source_offset = 0;
                }
            }
            _ => break,
        }
    }

    Ok(total_copied)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::buf::chunks::{
        SingleChunk,
        SingleChunkMut,
    };

    #[test]
    fn fails_if_source_longer_than_destination() {
        let mut destination = [0; 4];
        let source = [1; 16];

        match copy(&mut destination, .., &source, ..) {
            Ok(_) => panic!("copy didn't fail"),
            Err(CopyError::Full(Full { required, buf_length })) => {
                assert_eq!(required, 16);
                assert_eq!(buf_length, 4);
            }
            Err(e) => panic!("copy failed with incorrect error: {e:?}"),
        }
    }

    #[test]
    fn fails_if_source_shorter_than_destination() {
        let mut destination = [0; 16];
        let source = [1; 4];

        match copy(&mut destination, .., &source, ..) {
            Ok(_) => panic!("copy didn't fail"),
            Err(CopyError::LengthMismatch(LengthMismatch {
                destination_range,
                destination_length,
                source_range,
                source_length,
            })) => {
                assert_eq!(destination_range, Range::from(..));
                assert_eq!(source_range, Range::from(..));
                assert_eq!(destination_length, 16);
                assert_eq!(source_length, 4);
            }
            Err(e) => panic!("copy failed with incorrect error: {e:?}"),
        }
    }

    #[test]
    fn fails_if_source_range_shorter_than_destination_range() {
        let mut destination = [0; 16];
        let source = [1; 16];

        match copy(&mut destination, .., &source, ..4) {
            Ok(_) => panic!("copy didn't fail"),
            Err(CopyError::LengthMismatch(LengthMismatch {
                destination_range,
                destination_length,
                source_range,
                source_length,
            })) => {
                assert_eq!(destination_range, Range::from(..));
                assert_eq!(source_range, Range::from(..4));
                assert_eq!(destination_length, 16);
                assert_eq!(source_length, 4);
            }
            Err(e) => panic!("copy failed with incorrect error: {e:?}"),
        }
    }

    #[test]
    fn fails_if_source_range_longer_than_destination_range() {
        let mut destination = [0; 16];
        let source = [1; 16];

        match copy(&mut destination, ..4, &source, ..) {
            Ok(_) => panic!("copy didn't fail"),
            Err(CopyError::LengthMismatch(LengthMismatch {
                destination_range,
                destination_length,
                source_range,
                source_length,
            })) => {
                assert_eq!(destination_range, Range::from(..4));
                assert_eq!(source_range, Range::from(..));
                assert_eq!(destination_length, 4);
                assert_eq!(source_length, 16);
            }
            Err(e) => panic!("copy failed with incorrect error: {e:?}"),
        }
    }

    #[test]
    fn copy_full_range() {
        let mut destination = [42; 16];
        let mut source = [0; 16];
        for i in 0..16 {
            source[i] = i as u8;
        }
        let expected = source;

        match copy(&mut destination, .., source, ..) {
            Err(e) => panic!("copy failed: {e:?}"),
            Ok(total_copied) => {
                assert_eq!(total_copied, 16);
                assert_eq!(expected, destination);
            }
        }
    }

    #[test]
    fn copy_single_chunk_full_range() {
        let mut destination = [42; 16];
        let mut source = [0; 16];
        for i in 0..16 {
            source[i] = i as u8;
        }
        let expected = source;

        let mut dest_chunks = Peekable::new(SingleChunkMut::new(&mut destination));
        let mut src_chunks = Peekable::new(SingleChunk::new(&source));

        let mut dest_offset = 0;
        let mut src_offset = 0;

        match copy_chunks(
            &mut dest_chunks,
            &mut dest_offset,
            &mut src_chunks,
            &mut src_offset,
            16,
        ) {
            Err(e) => panic!("copy_chunks failed: {e:?}"),
            Ok(total_copied) => {
                assert_eq!(dest_offset, 0);
                assert_eq!(src_offset, 0);
                assert_eq!(total_copied, 16);
                assert_eq!(dest_chunks.peek(), None);
                assert_eq!(src_chunks.peek(), None);
                assert_eq!(expected, destination);
            }
        }
    }

    #[test]
    fn copy_single_chunk_partially() {
        let mut destination = [42; 16];
        let mut source = [0; 16];
        let mut expected = destination;
        for i in 0..16 {
            source[i] = i as u8;
        }
        expected[1..9].copy_from_slice(&source[4..12]);

        let mut dest_chunks = Peekable::new(SingleChunkMut::new(&mut destination));
        let mut src_chunks = Peekable::new(SingleChunk::new(&source));

        let mut dest_offset = 1;
        let mut src_offset = 4;

        match copy_chunks(
            &mut dest_chunks,
            &mut dest_offset,
            &mut src_chunks,
            &mut src_offset,
            8,
        ) {
            Err(e) => panic!("copy_chunks failed: {e:?}"),
            Ok(total_copied) => {
                assert_eq!(dest_offset, 9);
                assert_eq!(src_offset, 12);
                assert_eq!(total_copied, 8);
                assert!(dest_chunks.peek().is_some());
                assert_eq!(src_chunks.peek(), Some(&source.as_ref()));
                assert_eq!(expected, destination);
            }
        }
    }
}

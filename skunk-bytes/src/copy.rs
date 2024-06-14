use super::{
    Buf,
    BufMut,
    Range,
    RangeOutOfBounds,
};
use crate::{
    buf::chunks::NonEmptyIter,
    util::Peekable,
};

/// Error while copying from a [`Buf`] to a [`BufMut`].
#[derive(Debug, PartialEq, Eq, thiserror::Error)]
pub enum CopyError {
    #[error("Destination index out of bounds")]
    DestinationRangeOutOfBounds(RangeOutOfBounds),

    #[error("Source index out of bounds")]
    SourceRangeOutOfBounds(RangeOutOfBounds),

    #[error("Length mismatch: destination ({destination_length}) != source ({source_length})")]
    LengthMismatch {
        destination_range: Range,
        destination_length: usize,
        source_range: Range,
        source_length: usize,
    },
}

/// Copies bytes from `source` to `destination` with respective ranges.
///
/// This can fail if either range is out of bounds, or the lengths of both
/// ranges aren't equal. See [`CopyError`].
///
/// # Todo
///
/// Either modify this or create a similar function that will grow the
/// destination buffer as necessary (see `write_helper`).
pub fn copy(
    mut destination: impl BufMut,
    destination_range: impl Into<Range>,
    source: impl Buf,
    source_range: impl Into<Range>,
) -> Result<(), CopyError> {
    let destination_range = destination_range.into();
    let source_range = source_range.into();
    let destination_length = destination_range.len_in(0, destination.len());
    let source_length = source_range.len_in(0, source.len());

    if destination_length != source_length {
        return Err(CopyError::LengthMismatch {
            destination_range,
            destination_length,
            source_range,
            source_length,
        });
    }

    let mut destination_chunks = Peekable::new(NonEmptyIter(
        destination
            .chunks_mut(destination_range)
            .map_err(CopyError::DestinationRangeOutOfBounds)?,
    ));
    let mut source_chunks = Peekable::new(NonEmptyIter(
        source
            .chunks(source_range)
            .map_err(CopyError::SourceRangeOutOfBounds)?,
    ));

    let copy_result = copy_chunks(
        &mut destination_chunks,
        0,
        &mut source_chunks,
        0,
        source_length,
    )?;

    // write a test for this
    assert_eq!(
        copy_result.total_copied, source_length,
        "Expected total amount of bytes copied to be equal to the destination and source length."
    );
    assert!(
        destination_chunks.next().is_none(),
        "Expected destination chunk iterator to be exhausted."
    );
    assert!(
        source_chunks.next().is_none(),
        "Expected source chunk iterator to be exhausted."
    );

    Ok(())
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
pub fn copy_chunks<'a, D: Iterator<Item = &'a mut [u8]>, S: Iterator<Item = &'a [u8]>>(
    destination_chunks: &mut Peekable<D>,
    mut destination_offset: usize,
    source_chunks: &mut Peekable<S>,
    mut source_offset: usize,
    mut amount: usize,
) -> Result<CopyChunksResult, CopyError> {
    let mut total_copied = 0;

    while amount > 0 {
        match (destination_chunks.peek_mut(), source_chunks.peek()) {
            (Some(dest_chunk), Some(src_chunk)) => {
                let n = std::cmp::min(
                    std::cmp::min(
                        dest_chunk.len() - destination_offset,
                        src_chunk.len() - source_offset,
                    ),
                    amount,
                );

                dest_chunk[destination_offset..][..n]
                    .copy_from_slice(&src_chunk[source_offset..][..n]);

                destination_offset += n;
                source_offset += n;
                total_copied += n;
                amount -= n;

                if destination_offset == dest_chunk.len() {
                    destination_chunks.next();
                    destination_offset = 0;
                }
                if source_offset == src_chunk.len() {
                    source_chunks.next();
                    source_offset = 0;
                }
            }
            _ => break,
        }
    }

    Ok(CopyChunksResult {
        destination_offset,
        source_offset,
        total_copied,
    })
}

/// Result of [`copy_chunks`]
#[derive(Debug)]
pub struct CopyChunksResult {
    /// Offset into `destination_chunk` after copying.
    pub destination_offset: usize,

    /// Offset into `source_chunk` after copying.
    pub source_offset: usize,

    /// Total number of bytes copied between chunk iterators.
    pub total_copied: usize,
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
            Err(CopyError::LengthMismatch {
                destination_range,
                destination_length,
                source_range,
                source_length,
            }) => {
                assert_eq!(destination_range, Range::from(..));
                assert_eq!(source_range, Range::from(..));
                assert_eq!(destination_length, 4);
                assert_eq!(source_length, 16);
            }
            Err(e) => panic!("copy failed with incorrect error: {e}"),
        }
    }

    #[test]
    fn fails_if_source_shorter_than_destination() {
        let mut destination = [0; 16];
        let source = [1; 4];

        match copy(&mut destination, .., &source, ..) {
            Ok(_) => panic!("copy didn't fail"),
            Err(CopyError::LengthMismatch {
                destination_range,
                destination_length,
                source_range,
                source_length,
            }) => {
                assert_eq!(destination_range, Range::from(..));
                assert_eq!(source_range, Range::from(..));
                assert_eq!(destination_length, 16);
                assert_eq!(source_length, 4);
            }
            Err(e) => panic!("copy failed with incorrect error: {e}"),
        }
    }

    #[test]
    fn fails_if_source_range_longer_than_destination_range() {
        let mut destination = [0; 16];
        let source = [1; 16];

        match copy(&mut destination, .., &source, ..4) {
            Ok(_) => panic!("copy didn't fail"),
            Err(CopyError::LengthMismatch {
                destination_range,
                destination_length,
                source_range,
                source_length,
            }) => {
                assert_eq!(destination_range, Range::from(..));
                assert_eq!(source_range, Range::from(..4));
                assert_eq!(destination_length, 16);
                assert_eq!(source_length, 4);
            }
            Err(e) => panic!("copy failed with incorrect error: {e}"),
        }
    }

    #[test]
    fn fails_if_source_range_shorter_than_destination_range() {
        let mut destination = [0; 16];
        let source = [1; 16];

        match copy(&mut destination, ..4, &source, ..) {
            Ok(_) => panic!("copy didn't fail"),
            Err(CopyError::LengthMismatch {
                destination_range,
                destination_length,
                source_range,
                source_length,
            }) => {
                assert_eq!(destination_range, Range::from(..4));
                assert_eq!(source_range, Range::from(..));
                assert_eq!(destination_length, 4);
                assert_eq!(source_length, 16);
            }
            Err(e) => panic!("copy failed with incorrect error: {e}"),
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
            Err(e) => panic!("copy failed: {e}"),
            Ok(()) => {
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

        match copy_chunks(&mut dest_chunks, 0, &mut src_chunks, 0, 16) {
            Err(e) => panic!("copy_chunks failed: {e}"),
            Ok(CopyChunksResult {
                destination_offset,
                source_offset,
                total_copied,
            }) => {
                assert_eq!(destination_offset, 0);
                assert_eq!(source_offset, 0);
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

        match copy_chunks(&mut dest_chunks, 1, &mut src_chunks, 4, 8) {
            Err(e) => panic!("copy_chunks failed: {e}"),
            Ok(CopyChunksResult {
                destination_offset,
                source_offset,
                total_copied,
            }) => {
                assert_eq!(destination_offset, 9);
                assert_eq!(source_offset, 12);
                assert_eq!(total_copied, 8);
                assert!(dest_chunks.peek().is_some());
                assert_eq!(src_chunks.peek(), Some(&source.as_ref()));
                assert_eq!(expected, destination);
            }
        }
    }
}

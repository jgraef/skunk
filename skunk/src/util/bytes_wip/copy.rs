use super::{
    Buf,
    BufMut,
    Range,
    RangeOutOfBounds,
};
use crate::util::{
    bytes_wip::buf::NonEmptyIter,
    Peekable,
};

/// Error while copying from a [`Buf`] to a [`BufMut`].
#[derive(Debug, thiserror::Error)]
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
/// destination buffer as necessary.
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

/// Copies 'amount' bytes from `source_chunks` to `destination_chunks` with the
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

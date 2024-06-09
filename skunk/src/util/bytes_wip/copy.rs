use super::{
    Buf,
    BufMut,
    Range,
    RangeOutOfBounds,
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
/// ranges doesn't match up. See [`CopyError`].
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

    let mut dest_chunks = destination
        .chunks_mut(destination_range)
        .map_err(CopyError::DestinationRangeOutOfBounds)?;
    let mut src_chunks = source
        .chunks(source_range)
        .map_err(CopyError::SourceRangeOutOfBounds)?;

    let mut current_dest_chunk: Option<&mut [u8]> = dest_chunks.next();
    let mut current_src_chunk: Option<&[u8]> = src_chunks.next();

    let mut dest_pos = 0;
    let mut src_pos = 0;

    loop {
        match (&mut current_dest_chunk, current_src_chunk) {
            (None, None) => break Ok(()),
            (Some(dest_chunk), Some(src_chunk)) => {
                let n = std::cmp::min(dest_chunk.len() - dest_pos, src_chunk.len() - src_pos);

                dest_chunk[dest_pos..][..n].copy_from_slice(&src_chunk[src_pos..][..n]);

                dest_pos += n;
                src_pos += n;

                if dest_pos == dest_chunk.len() {
                    current_dest_chunk = dest_chunks.next();
                    dest_pos = 0;
                }
                if src_pos == src_chunk.len() {
                    current_src_chunk = src_chunks.next();
                    dest_pos = 0;
                }
            }
            (Some(dest_chunk), None) => {
                if dest_chunk.is_empty() {
                    current_dest_chunk = dest_chunks.next();
                }
                else {
                    panic!("destination not full");
                }
            }
            (None, Some(src_chunk)) => {
                if src_chunk.is_empty() {
                    current_src_chunk = src_chunks.next();
                }
                else {
                    panic!("source not exhaused");
                }
            }
        }
    }
}

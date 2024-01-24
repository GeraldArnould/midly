use crate::smf::{Chunk, ChunkIter};
use crate::{prelude::*, TrackIter};

#[derive(Debug, Clone)]
pub struct Ots<'a>(pub TrackIter<'a>);

impl<'a> Ots<'a> {
    // get the first OTS section from a ChunkIter, additional ones are ignored.
    pub(crate) fn parse(chunk_iter: ChunkIter<'a>) -> Result<Option<Self>> {
        let mut ots_iter = chunk_iter.filter(|c| matches!(c, Ok(Chunk::Ots(..))));
        let ots = match ots_iter.next() {
            Some(maybe_chunk) => match maybe_chunk.context(err_invalid!("invalid OTS header"))? {
                Chunk::Ots(data) => Ok(data),
                _ => Err(err_invalid!("expected OTS found another type of chunk")),
            },
            None => return Ok(None),
        }?;

        let tracks = TrackIter::new(ots);
        Ok(Some(Ots(tracks)))
    }
}

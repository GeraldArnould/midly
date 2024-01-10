use crate::prelude::*;
use crate::smf::{Chunk, ChunkIter};

pub struct Mh<'a>(&'a [u8]);

impl<'a> Mh<'a> {
    // get the first MH section from a ChunkIter, additional ones are ignored.
    pub(crate) fn parse(chunk_iter: ChunkIter<'a>) -> Result<Option<Self>> {
        let mut mh_iter = chunk_iter.filter(|c| matches!(c, Ok(Chunk::Mh(..))));
        let mh = match mh_iter.next() {
            Some(maybe_chunk) => match maybe_chunk.context(err_invalid!("invalid MH header"))? {
                Chunk::Mh(data) => Ok(data),
                _ => Err(err_invalid!("expected MH found another type of chunk")),
            },
            None => return Ok(None),
        }?;
        Ok(Some(Mh(mh)))
    }
}

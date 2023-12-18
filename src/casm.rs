use crate::prelude::*;
use crate::smf::{Chunk, ChunkIter};

pub(crate) struct Casm<'a>(&'a [u8]);

impl<'a> Casm<'a> {
    // get the first CASM section from a ChunkIter, additional ones are ignored.
    pub(crate) fn parse(chunk_iter: ChunkIter<'a>) -> Result<Option<Self>> {
        let mut casm_iter = chunk_iter
            .filter(|c| matches!(c, Ok(Chunk::Casm(..))));
        let casm = match casm_iter.next() {
            Some(maybe_chunk) => match maybe_chunk.context(err_invalid!("invalid midi header"))? {
                Chunk::Casm(data) => Ok(data),
                _ => Err(err_invalid!("expected CASM found another type of chunk")),
            },
            None => return Ok(None),
        }?;
        Ok(Some(Casm(casm)))
    }
}

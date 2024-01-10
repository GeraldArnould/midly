use crate::prelude::*;
use crate::smf::{Chunk, ChunkIter};

#[derive(Debug)]
pub struct Mdb<'a>(pub(crate) RecordIter<'a>);

impl<'a> Mdb<'a> {
    // get the first MDB section from a ChunkIter, additional ones are ignored.
    pub(crate) fn parse(chunk_iter: ChunkIter<'a>) -> Result<Option<Mdb>> {
        let mut mdb_iter = chunk_iter
            .filter(|c| matches!(c, Ok(Chunk::Mdb(..))));
        let mdb = match mdb_iter.next() {
            Some(maybe_chunk) => match maybe_chunk.context(err_invalid!("invalid MDB header"))? {
                Chunk::Mdb(data) => Ok(data),
                _ => Err(err_invalid!("expected MDB found another type of chunk")),
            },
            None => return Ok(None),
        }?;
        let inner = ChunkIter::new(mdb);
        Ok(Some(Mdb(RecordIter{ inner })))
    }
}

#[derive(Debug)]
pub(crate) struct Record {
    /// Tempo of the tune in ms / quarter-note
    tempo: u24,
    /// Time signature
    signature: Signature,
    /// Song's title
    // chunk: Id::SongTitleData,
    title: String,
    /// Song's genre
    // chunk: Id::GenreTitleData,
    genre: String,
    /// Keyword associated with the song
    // chunk: Id::Keyword1
    keyword1: Option<String>,
    /// Keyword associated with the song
    // chunk: Id::Keyword2
    keyword2: Option<String>,
}

impl Record {
    fn read(chunk: Chunk) -> Result<Record> {
        let mut value = match chunk {
            Chunk::Record(v) => v,
            _ => bail!(err_invalid!("not a Record chunk")),
        };

        let tempo = u24::read(&mut value)?;
        // Signature
        let upper = u8::read(&mut value)?;
        let lower = u8::read(&mut value)?;

        // The rest of the data is chunks
        let chunk_iter = ChunkIter::new(value);
        // Chunks should be in order Song Title, Genre Name, Keyword1, Keyword2
        // We'll just process the iterator and get values as they come to deal with
        // malformed files.
        let mut title = String::default();
        let mut genre = String::default();
        let mut keyword1: Option<String> = None;
        let mut keyword2: Option<String> = None;
        for chunk in chunk_iter {
            match chunk {
                Ok(Chunk::SongTitleData(t)) => title = match std::str::from_utf8(t) {
                    Ok(val) => val.to_string(),
                    Err(_) => String::default(),
                },
                Ok(Chunk::GenreTitleData(t)) => genre = match std::str::from_utf8(t) {
                    Ok(val) => val.to_string(),
                    Err(_) => String::default(),
                },
                Ok(Chunk::Keyword1(t)) => keyword1 = match std::str::from_utf8(t) {
                    Ok(val) if !val.is_empty() => Some(val.to_string()),
                    Ok(_) => None,
                    Err(_) => None,
                },
                Ok(Chunk::Keyword2(t)) => keyword2 = match std::str::from_utf8(t) {
                    Ok(val) if !val.is_empty() => Some(val.to_string()),
                    Ok(_) => None,
                    Err(_) => None,
                },
                Err(_) => Err(err_malformed!("failed to read chunk"))?,
                _ => (),
            }
        };
        Ok(Record {tempo, signature: Signature {upper, lower}, title, genre, keyword1, keyword2})
    }
}

#[derive(Debug)]
pub(crate) struct RecordIter<'a> {
    inner: ChunkIter<'a>,
}

impl<'a> Iterator for RecordIter<'a> {
    type Item = Result<Record>;
    fn next(&mut self) -> Option<Self::Item> {
       let chunk = self.inner.next()?;
        match chunk {
            Ok(c) if matches!(c, Chunk::Record(..)) => match Record::read(c) {
                Ok(record) => Some(Ok(record)),
                Err(err) => if cfg!(feature = "strict") {
                    Some(Err(err).context(err_invalid!("invalid Record")))
                } else {
                    None
                }
            },
            // Wrong chunk type
            Ok(_) => None,
            Err(err) => if cfg!(feature = "strict") {
                Some(Err(err).context(err_malformed!("malformed Record")))
            } else {
                None
            },
        }
    }
}

/// Time signature as a fraction, like in normal musical notation
#[derive(Debug, PartialEq)]
pub(crate) struct Signature {
    /// How many notes per bar
    upper: u8,
    /// note being counted
    lower: u8,
}



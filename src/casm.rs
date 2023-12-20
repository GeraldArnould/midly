use crate::ctab::Ctab;
use crate::Error;
use crate::prelude::*;
use crate::smf::{Chunk, ChunkIter};

pub struct Casm<'a>(pub(crate) CsegIter<'a>);

impl<'a> Casm<'a> {
    // get the first CASM section from a ChunkIter, additional ones are ignored.
    pub(crate) fn parse(chunk_iter: ChunkIter<'a>) -> Result<Option<Self>> {
        let mut casm_iter = chunk_iter
            .filter(|c| matches!(c, Ok(Chunk::Casm(..))));
        // Take only the first CASM section found if any
        let casm = match casm_iter.next() {
            Some(maybe_chunk) => match maybe_chunk.context(err_invalid!("invalid CASM header"))? {
                Chunk::Casm(data) => Ok(data),
                _ => Err(err_invalid!("expected CASM found another type of chunk")),
            },
            None => return Ok(None),
        }?;

        Ok(Some(Casm(CsegIter { inner: ChunkIter::new(casm)})))
    }
}

#[derive(Debug)]
pub(crate) struct Cseg {
    style_parts: Vec<StylePart>,
    ctab: Vec<Ctab>,
}

impl Cseg {
    fn read(chunk: Chunk) -> Result<Cseg> {
        let value = match chunk {
            Chunk::Cseg(v) => v,
            _ => bail!(err_invalid!("not a CSEG chunk")),
        };

        // Following sections are chunks
        let mut chunk_iter = ChunkIter::new(value);
        let mut style_parts: Vec<StylePart> = vec![];
        let mut ctab: Vec<Ctab> = vec![];
        while let Some(chunk) = chunk_iter.next() {
            match chunk {
                Ok(Chunk::Sdec(data)) => {
                    // Style parts are separated by ',' (0x2C)
                    let parts = &mut data.split(|b| *b == 0x2C_u8);
                    for maybe_parts in parts {
                        match StylePart::try_from(maybe_parts) {
                            Ok(part) => style_parts.push(part),
                            Err(_) => Err(err_malformed!("could not read style part value"))?,
                        };
                    }
                },
                Ok(Chunk::Ctab1(data)) => {},
                Ok(Chunk::Ctab2(data)) => {},
                Ok(Chunk::Cntt(data)) => {},
                Ok(c) => Err(err_invalid!("found a chunk not belonging in a CASM section"))?,
                Err(err) => Err(err_invalid!("could not read chunk"))?,
            }
        };
        Ok(Cseg{style_parts, ctab})
    }
}

pub(crate) struct CsegIter<'a> {
    inner: ChunkIter<'a>,
}

impl<'a> Iterator for CsegIter<'a> {
    type Item = Result<Cseg>;
    fn next(&mut self) -> Option<Self::Item> {
        let chunk = self.inner.next()?;
        match chunk {
            Ok(c) if matches!(c, Chunk::Cseg(..)) => match Cseg::read(c) {
                Ok(cseg) => Some(Ok(cseg)),
                Err(err) => if cfg!(feature = "strict") {
                    Some(Err(err).context(err_invalid!("invalid CSEG")))
                } else {
                    None
                }
            },
            // Wrong chunk type
            Ok(_) => None,
            Err(err) => if cfg!(feature = "strict") {
                Some(Err(err).context(err_malformed!("malformed CSEG")))
            } else {
                None
            },
        }
    }
}

/// Known style sections
///
/// [StylePart::IntroD] and [StylePart::EndingD] are only available for the PSR-2000
/// [StylePart::FillInBA] corresponds to the "Break" section
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum StylePart {
    IntroA,
    IntroB,
    IntroC,
    IntroD,
    MainA,
    MainB,
    MainC,
    MainD,
    FillInAA,
    FillInBB,
    FillInCC,
    FillInDD,
    FillInBA,
    EndingA,
    EndingB,
    EndingC,
    EndingD,
}

impl TryFrom<&str> for StylePart {
    type Error = Error;

    fn try_from(value: &str) -> Result<Self> {
        // TODO try matching on lowercase value to be more resilient against malformed files
        match value {
            "Intro A" => Ok(StylePart::IntroA),
            "Intro B" => Ok(StylePart::IntroB),
            "Intro C" => Ok(StylePart::IntroC),
            "Intro D" => Ok(StylePart::IntroD),
            "Main A" => Ok(StylePart::MainA),
            "Main B" => Ok(StylePart::MainB),
            "Main C" => Ok(StylePart::MainC),
            "Main D" => Ok(StylePart::MainD),
            "Fill In AA" => Ok(StylePart::FillInAA),
            "Fill In BB" => Ok(StylePart::FillInBB),
            "Fill In CC" => Ok(StylePart::FillInCC),
            "Fill In DD" => Ok(StylePart::FillInDD),
            "Fill In BA" => Ok(StylePart::FillInBA),
            "Ending A" => Ok(StylePart::EndingA),
            "Ending B" => Ok(StylePart::EndingB),
            "Ending C" => Ok(StylePart::EndingC),
            "Ending D" => Ok(StylePart::EndingD),
            _ => bail!(err_invalid!("invalid style part")),
        }
    }
}

impl<'a> From<StylePart> for &'a str {
    fn from(value: StylePart) -> &'a str {
        match value {
            StylePart::IntroA => "Intro A",
            StylePart::IntroB => "Intro B",
            StylePart::IntroC => "Intro C",
            StylePart::IntroD => "Intro D",
            StylePart::MainA => "Main A",
            StylePart::MainB => "Main B",
            StylePart::MainC => "Main C",
            StylePart::MainD => "Main D",
            StylePart::FillInAA => "Fill In AA",
            StylePart::FillInBB => "Fill In BB",
            StylePart::FillInCC => "Fill In CC",
            StylePart::FillInDD => "Fill In DD",
            StylePart::FillInBA => "Fill In BA",
            StylePart::EndingA => "Ending A",
            StylePart::EndingB => "Ending B",
            StylePart::EndingC => "Ending C",
            StylePart::EndingD => "Ending D",
        }
    }
}

impl<'a> TryFrom<&'a [u8]> for StylePart {
    type Error = Error;

    fn try_from(value: &'a [u8]) -> Result<Self> {
        // TODO try matching on lowercase value to be more resilient against malformed files
        match value {
            b"Intro A" => Ok(StylePart::IntroA),
            b"Intro B" => Ok(StylePart::IntroB),
            b"Intro C" => Ok(StylePart::IntroC),
            b"Intro D" => Ok(StylePart::IntroD),
            b"Main A" => Ok(StylePart::MainA),
            b"Main B" => Ok(StylePart::MainB),
            b"Main C" => Ok(StylePart::MainC),
            b"Main D" => Ok(StylePart::MainD),
            b"Fill In AA" => Ok(StylePart::FillInAA),
            b"Fill In BB" => Ok(StylePart::FillInBB),
            b"Fill In CC" => Ok(StylePart::FillInCC),
            b"Fill In DD" => Ok(StylePart::FillInDD),
            b"Fill In BA" => Ok(StylePart::FillInBA),
            b"Ending A" => Ok(StylePart::EndingA),
            b"Ending B" => Ok(StylePart::EndingB),
            b"Ending C" => Ok(StylePart::EndingC),
            b"Ending D" => Ok(StylePart::EndingD),
            _ => bail!(err_invalid!("invalid style part")),
        }
    }
}

impl<'a> From<StylePart> for &'a [u8] {
    fn from(value: StylePart) -> &'a [u8] {
        match value {
            StylePart::IntroA => b"Intro A",
            StylePart::IntroB => b"Intro B",
            StylePart::IntroC => b"Intro C",
            StylePart::IntroD => b"Intro D",
            StylePart::MainA => b"Main A",
            StylePart::MainB => b"Main B",
            StylePart::MainC => b"Main C",
            StylePart::MainD => b"Main D",
            StylePart::FillInAA => b"Fill In AA",
            StylePart::FillInBB => b"Fill In BB",
            StylePart::FillInCC => b"Fill In CC",
            StylePart::FillInDD => b"Fill In DD",
            StylePart::FillInBA => b"Fill In BA",
            StylePart::EndingA => b"Ending A",
            StylePart::EndingB => b"Ending B",
            StylePart::EndingC => b"Ending C",
            StylePart::EndingD => b"Ending D",
        }
    }
}

use core::convert::TryInto;
use std::collections::HashMap;
use crate::Error;
use crate::prelude::*;
use crate::smf::Chunk;

// Size of the various sections found in a CTAB chunk
const COMMON_SIZE: usize = 20;
const TABLE_SIZE: usize = 6;
const CTAB1_SIZE: usize = TABLE_SIZE;
const CTAB1_SPECIAL_SIZE: usize = 5;
const CTAB2_SIZE: usize = 27;
const CTAB2_SPECIAL_SIZE: usize = 7;
const CNTT_SIZE: usize = 2;

/// There are two types of CTAB chunks:
/// - Ctab1: oldest. May be associated with a CNTT chunk.
/// - Ctab2: All in one. No CNTT.
/// Ctab1 and Ctab2 share the same structure for their first 20 bytes.
/// An additional variant may be present in SFFv2: [`Version::Guitar`].
#[derive(PartialEq, Clone, Copy)]
pub(crate) enum Version {
    Ctab1,
    Ctab2,
    // Implies Ctab2
    Guitar,
}

#[derive(Debug)]
pub(crate) struct Ctab<'a> {
    /// Midi source channel: 0x00 (channel 1) to 0x0F (channel 16)
    source: u4,
    // name is padded with spaces (0x20) if smaller than 8 bytes
    name: String, // [u8; 8] in the raw bytes file.
    /// Accompaniment midi channel: must be in \[Ch9..Ch16\]
    /// * Ch9: Sub-rhythm
    /// * Ch10: Rhythm
    /// * Ch11: Bass
    /// * Ch12: Chord 1
    /// * Ch13: Chord 2
    /// * Ch14: Pad
    /// * Ch15: Phrase 1
    /// * Ch16: Phrase 2
    dest: u4,
    /// Whether the source channel data is editable (0x00) or not (0x01)
    editable: bool,
    /// Chords whose root note's is set to `true` here will mute the track.
    ///
    /// Specific notes are stored as bit values, MSB format.
    /// Bit value 1: chord will play the track, 0: chord will mute the track.
    /// The values in this field are inverted compared to the bits values: (1 -> false, 2 -> true)
    /// First byte (bits 7..4 are unused and always 0): \[ 0, 0, 0, 0, B, B♭, A, G# \]
    /// Second byte: \[ G, F#, F, E, E♭, D, C#, C \]
    note_mute: HashMap<Key, bool>,
    /// Specific chords mute the associated melody when played if [`chord_mute`] is true for this
    /// chord.
    chord_mute: HashMap<Chord, bool>,
    /// Key of the source channel
    source_chord: Key,
    /// Type of chord of the source channel
    source_chord_type: Chord,
    /// Note transposition tables
    /// SFFv2 splits the note's range into three sections, low, mid and high and has a separate
    /// set of tables for each section.
    /// SFFv1 has only one set of table for the whole note's range.
    table: Vec<Table>,
    /// lowest and highest notes of the middle range (inclusive). Only usefull for SFFv2.
    range: (u7, u7),
    /// the meaning of those bytes is not known
    special: Option<&'a [u8]>,
}

impl Ctab<'_> {
    pub(crate) fn read(chunk: Chunk) -> Result<Ctab> {
        let version: Version;
        let mut value = match chunk {
            Chunk::Ctab1(v) => { version = Version::Ctab1; v }
            Chunk::Ctab2(v) => { version = Version::Ctab2; v },
            _ => bail!(err_invalid!("not a CTAB type chunk")),
        };

        let source = u4::read(&mut value)?;
        let name = match value.split_checked(8) {
            Some(v) => match std::str::from_utf8(v) {
                Ok(name) => name.trim().to_string(),
                Err(_) => if cfg!(feature = "strict") {
                    bail!(err_malformed!("not a valid string for name")); 
                } else {
                        String::default()
                    },
            }
            None => bail!(err_invalid!("name field is not a string")),
        };
        let dest = u4::read(&mut value)?;
        let editable = u8::read(&mut value)? == 0;
        let data = [u8::read(&mut value)?, u8::read(&mut value)?];
        let note_mute = Ctab::read_note_mute(data)?;
        let data = match value.split_checked(5) {
            Some(v) => v.try_into().expect("array of size 5"),
            None => bail!(err_invalid!("not enough data for chord mute")),
        };
        let chord_mute = Ctab::read_chord_mute(data)?;
        let source_chord = Key::try_from(u8::read(&mut value)?)?;
        let source_chord_type = Chord::try_from(u8::read(&mut value)?)?;

        // table has at most 3 components
        let mut table = Vec::with_capacity(3);
        // full midi note's range by default for CTABv1
        let mut range = (u7::from(0), u7::from(127));
        let special;
        match version {
            Version::Ctab2 | Version::Guitar => {
                range = ( u7::read(&mut value)?, u7::read(&mut value)?);
                if let Some(data) = value.split_checked(TABLE_SIZE * 3) {
                    let low = Table::try_from((&data[..TABLE_SIZE], Version::Ctab2))?;
                    table.push(low);
                    let mid = Table::try_from((&data[TABLE_SIZE..TABLE_SIZE * 2], Version::Ctab2))?;
                    table.push(mid);
                    let high = Table::try_from((&data[TABLE_SIZE * 2..TABLE_SIZE * 3], Version::Ctab2))?;
                    table.push(high);
                } else {
                    bail!(err_malformed!("cannot construct transposition table"));
                }

                special = value.split_checked(CTAB2_SPECIAL_SIZE);
                if special.is_none() && cfg!(feature = "strict") {
                    bail!(err_malformed!("missing special bytes at the end of CTABv2"));
                }
            },
            Version::Ctab1 => {
                if let Some(data) = value.split_checked(TABLE_SIZE) {
                    table.push(Table::try_from((data, Version::Ctab1))?);
                } else {
                    bail!(err_malformed!("cannot construct transposition table"));
                }
                
                if u8::read(&mut value)? != 0x00 {
                    special = value.split_checked(CTAB1_SPECIAL_SIZE - 1);
                    if special.is_none() && cfg!(feature = "strict") {
                        bail!(err_malformed!("missing special bytes at the end of CTABv1"));
                    }
                } else {
                    special = None;
                }
            }
        }

        Ok(Ctab { source, name, dest, editable, note_mute, chord_mute, source_chord,
            source_chord_type, table, range, special })
    }

    fn read_note_mute(value: [u8; 2]) -> Result<HashMap<Key, bool>> {
        // The 4 MSB of the first byte are always 0.
        if value[0] > 0b1111 && cfg!(feature = "strict") {
            bail!(err_malformed!("note mute first nibble is not 0"));
        }
        let b = value[0] & 0b1000 == 0;
        let bb = value[0] & 0b0100 == 0;
        let a = value[0] & 0b0010 == 0;
        let gs = value[0] & 0b0001 == 0;

        // second byte.
        let g = value[1] & 0b1000_0000 == 0;
        let fs = value[1] & 0b0100_0000 == 0;
        let f = value[1] & 0b0010_0000 == 0;
        let e = value[1] & 0b0001_0000 == 0;
        let eb = value[1] & 0b0000_1000 == 0;
        let d = value[1] & 0b0000_0100 == 0;
        let cs = value[1] & 0b0000_0010 == 0;
        let c = value[1] & 0b0000_0001 == 0;

        Ok(HashMap::from([(Key::B, b), (Key::Bb, bb), (Key::A, a), (Key::Gs, gs), (Key::G, g), (Key::Fs, fs),
            (Key::F, f), (Key::E, e), (Key::Eb, eb), (Key::D, d), (Key::Cs, cs), (Key::C, c)]))
    }

    /// Any chord type set to false here will mute the track when played.
    ///
    /// Chord Mute is encoded across five bytes:
    /// * Byte 1 \[0x00 .. 0xOF\]:
    /// Bits 2 and 3 are only used for drums and percussions. When bit 2 is set to 1, auto play the
    /// drums from the start of the performance.
    ///     * bit 7 = 0 (unused)
    ///     * bit 6 = 0 (unused)
    ///     * bit 5 = 0 (unused)
    ///     * bit 4 = 0 (unused)
    ///     * bit 3 = ? (unknown)
    ///     * bit 2 = enable autostart
    ///     * bit 1 = 1+2+5
    ///     * bit 0 = sus4
    /// * Byte 2 \[0x00 .. 0xFF\]
    ///     * Bit 7 = 1+5
    ///     * Bit 6 = 1+8
    ///     * Bit 5 = 7aug
    ///     * Bit 4 = Maj7aug
    ///     * Bit 3 = 7(#9)
    ///     * Bit 2 = 7(b13)
    ///     * Bit 1 = 7(b9)
    ///     * Bit 0 = 7(13)
    /// * Byte 3 \[0x00 .. 0xFF\]
    ///     * Bit 7 = 7#11
    ///     * Bit 6 = 7(9)
    ///     * Bit 5 = 7b5
    ///     * Bit 4 = 7sus4
    ///     * Bit 3 = 7th
    ///     * Bit 2 = dim7
    ///     * Bit 1 = dim
    ///     * Bit 0 = minMaj7(9)
    /// * Byte 4 \[0x00 .. 0xFF\]
    ///     * Bit 7 = minMaj7
    ///     * Bit 6 = min7(11)
    ///     * Bit 5 = min7(9)
    ///     * Bit 4 = min(9)
    ///     * Bit 3 = min7b5
    ///     * Bit 2 = min7
    ///     * Bit 1 = min6
    ///     * Bit 0 = min
    /// * Byte 5 \[0x00 .. 0xFF\]
    ///     * Bit 7 = aug
    ///     * Bit 6 = Maj6(9)
    ///     * Bit 5 = Maj7(9)
    ///     * Bit 4 = Maj(9)
    ///     * Bit 3 = Maj7#11
    ///     * Bit 2 = Maj7
    ///     * Bit 1 = Maj6
    ///     * Bit 0 = Maj
    fn read_chord_mute(value: [u8; 5]) -> Result<HashMap<Chord, bool>> {
        let mut chord_mute: HashMap<Chord, bool> = HashMap::with_capacity(CHORD_SIZE);
        let chords_order = [
            // byte 0 (First nibble is 0x0)
            Chord::SpecialPercussion, Chord::SpecialAutostart, Chord::OnePlusTwoPlus5, Chord::Sus4,
            // byte 1
            Chord::OnePlusFive, Chord::OnePlusEight, Chord::SevenAug, Chord::Maj7aug,
            Chord::SevenS9, Chord::SevenB13, Chord::SevenB9, Chord::Seven13,
            // byte 2
            Chord::SevenS11, Chord::Seven9, Chord::SevenB5, Chord::SevenSus4,
            Chord::Seven, Chord::Dim7, Chord::Dim, Chord::MinMaj7_9,
            // byte 3
            Chord::MinMaj7, Chord::Min7_11, Chord::Min7_9, Chord::Min9,
            Chord::Min7b5, Chord::Min7, Chord::Min6, Chord::Min,
            // byte 4
            Chord::Aug, Chord::Maj6_9, Chord::Maj7_9, Chord::Maj9,
            Chord::Maj7s11, Chord::Maj7, Chord::Maj6, Chord::Maj,
        ];
        // The 4 MSB of the first byte are always 0.
        if value[0] > 0b1111 && cfg!(feature = "strict") {
            bail!(err_malformed!("first nibble of chord mute field is not 0"));
        }

        // iterates over 5 bytes, except the 4 first bits of the first byte.
        for (cur, chord) in chords_order.iter().enumerate() {
            // Cursor position within the current byte
            let pos = (cur + 4) % 8;
            // Current byte from `value`
            let cur_byte = (cur + 4) / 8;
            let mask = 1 << (8 - pos - 1);
            let not_muted = value[cur_byte] & mask != 0;
            // println!("cur: {:?}, pos: {:?}, cur_byte: {:?} mask: {:08b}, chord: {:?}/{:?}",
            // cur, pos, cur_byte, mask, chord, not_muted);
            chord_mute.insert(*chord, not_muted);
        }

        Ok(chord_mute)
    }
}

/// Standard keys used in style files
#[derive(Debug, PartialEq, Hash, Eq)]
pub enum Key {
    C,
    Cs,
    D,
    Eb,
    E,
    F,
    Fs,
    G,
    Gs,
    A,
    Bb,
    B,
}

impl TryFrom<u8> for Key {
    type Error = Error;

    fn try_from(value: u8) -> Result<Self> {
        Ok(
            match value {
                0x00 => Self::C,
                0x01 => Self::Cs,
                0x02 => Self::D,
                0x03 => Self::Eb,
                0x04 => Self::E,
                0x05 => Self::F,
                0x06 => Self::Fs,
                0x07 => Self::G,
                0x08 => Self::Gs,
                0x09 => Self::A,
                0x0A => Self::Bb,
                0x0B => Self::B,
                _ => bail!(err_invalid!("invalid key value")),
            }
        )
    }
}

// Number of variants in the Chord enum
const CHORD_SIZE: usize = 37;

/// Chords variants found in style files
#[derive(Debug, PartialEq, Eq, Hash, Copy, Clone)]
enum Chord {
    // TODO find more sensible chord names
    Maj,
    Maj6,
    Maj7,
    Maj7s11,
    Maj9,
    Maj7_9,
    Maj6_9,
    Aug,
    Min,
    Min6,
    Min7,
    Min7b5,
    Min9,
    Min7_9,
    Min7_11,
    MinMaj7,
    MinMaj7_9,
    Dim,
    Dim7,
    Seven,
    SevenSus4,
    SevenB5,
    Seven9,
    SevenS11,
    Seven13,
    SevenB9,
    SevenB13,
    SevenS9,
    Maj7aug,
    SevenAug,
    OnePlusEight,
    OnePlusFive,
    Sus4,
    OnePlusTwoPlus5,
    Cancel,
    SpecialAutostart,
    SpecialPercussion,
}

impl TryFrom<u8> for Chord {
    type Error = Error;

    fn try_from(value: u8) -> Result<Self> {
        Ok(
            match value {
                0x00 => Self::Maj,
                0x01 => Self::Maj6,
                0x02 => Self::Maj7,
                0x03 => Self::Maj7s11,
                0x04 => Self::Maj9,
                0x05 => Self::Maj7_9,
                0x06 => Self::Maj6_9,
                0x07 => Self::Aug,
                0x08 => Self::Min,
                0x09 => Self::Min6,
                0x0A => Self::Min7,
                0x0B => Self::Min7b5,
                0x0C => Self::Min9,
                0x0D => Self::Min7_9,
                0x0E => Self::Min7_11,
                0x0F => Self::MinMaj7,
                0x10 => Self::MinMaj7_9,
                0x11 => Self::Dim,
                0x12 => Self::Dim7,
                0x13 => Self::Seven,
                0x14 => Self::SevenSus4,
                0x15 => Self::SevenB5,
                0x16 => Self::Seven9,
                0x17 => Self::SevenS11,
                0x18 => Self::Seven13,
                0x19 => Self::SevenB9,
                0x1A => Self::SevenB13,
                0x1B => Self::SevenS9,
                0x1C => Self::Maj7aug,
                0x1D => Self::SevenAug,
                0x1E => Self::OnePlusEight,
                0x1F => Self::OnePlusFive,
                0x20 => Self::Sus4,
                0x21 => Self::OnePlusTwoPlus5,
                0x22 => Self::Cancel,
                // Byte range 0x00..=0x22
                _ => bail!(err_invalid!("unknown chord")),
            }
        )
    }
}


#[derive(Debug, PartialEq)]
pub(crate) enum RetriggerRule {
    Stop,
    PitchShift,
    PitchShiftToRoot,
    Retrigger,
    RetriggerToRoot,
    NoteGenerator,
}

impl TryFrom<u8> for RetriggerRule {
    type Error = Error;

    fn try_from(value: u8) -> Result<Self> {
        Ok(
            match value {
                0x00 => Self::Stop,
                0x01 => Self::PitchShift,
                0x02 => Self::PitchShiftToRoot,
                0x03 => Self::Retrigger,
                0x04 => Self::RetriggerToRoot,
                0x05 => Self::NoteGenerator,
                _ => bail!(err_invalid!("unknown retrigger rule")),
            }
        )
    }
}

#[derive(Debug, PartialEq, Default)]
pub(crate) enum TranspositionType {
    #[default]
    RootTransposition,
    RootFixed,
    Guitar,
}

impl TryFrom<(u8, Version)> for TranspositionType {
    type Error = Error;

    fn try_from(value: (u8, Version)) -> Result<Self> {
        let (value, version) = value;
        Ok(
            match value {
                0x00 => Self::RootTransposition,
                0x01 => Self::RootFixed,
                0x02 => { 
                    if version == Version::Ctab1 && cfg!(feature = "strict") {
                        bail!(err_invalid!("Guitar transposition mode in SFFv1"));
                    }
                    Self::Guitar
                },
                _ => {
                    if cfg!(feature = "strict") {
                        bail!(err_invalid!("unknown transposition mode"));
                    } else {
                        // Return default transposition
                        Self::default()
                    }
                },
            }
        )
    }
}

#[derive(Debug, PartialEq, Default)]
pub(crate) enum TranspositionTable {
    #[default]
    Bypass,
    Melody,
    Chord,
    MelodicMinor,
    HarmonicMinor,
    // Only for `Version::Ctab2`
    MelodicMinor5th,
    HarmonicMinor5th,
    NaturalMinor,
    NaturalMinor5th,
    Dorian,
    Dorian5th,
    // Only for `Version::Ctab1`
    Bass,
    // Only for NTR::Guitar (implies `Version::Ctab2`)
    AllPurpose,
    Stroke,
    Arpeggio,
}

impl TryFrom<(u8, Version)> for TranspositionTable {
    type Error = Error;

    fn try_from(value: (u8, Version)) -> Result<Self> {
        let (value, version) = value;
        // ignore most significant bit (bass on)
        let value = value & 0b0111_1111;
        Ok(
            match value {
                0x00 if version == Version::Guitar => Self::AllPurpose,
                0x00 => Self::Bypass,
                0x01 if version == Version::Guitar => Self::Stroke,
                0x01 => Self::Melody,
                0x02 if version == Version::Guitar => Self::Arpeggio,
                0x02 => Self::Chord,
                0x03 if version == Version::Ctab1 => Self::Bass,
                0x03 => Self::MelodicMinor,
                0x04 if version == Version::Ctab1 => Self::MelodicMinor,
                0x04 => Self::MelodicMinor5th,
                0x05 => Self::HarmonicMinor,
                _e if version == Version::Ctab1 && cfg!(feature = "strict") => bail!(err_invalid!("transposition table not valid in SFFv1")),
                0x06 => Self::HarmonicMinor5th,
                0x07 => Self::NaturalMinor,
                0x08 => Self::NaturalMinor5th,
                0x09 => Self::Dorian,
                0x0A => Self::Dorian5th,
                _e => if cfg!(feature = "strict") {
                    bail!(err_invalid!("unknown transposition table"));
                    } else {
                        Self::default()
                    }
            }
        )
    }
}

#[derive(Debug, PartialEq)]
pub(crate) struct Table {
    // Note Transposition Table
    pub(crate) ntr: TranspositionType,
    // Note Transposition Rule
    pub(crate) ntt: TranspositionTable,
    /// Whether bass mode is activated. Only relevant for [`Version::Ctab2`]
    bass_on: bool,
    /// Chords with a root higher than `high_key` are transposed to the octave below this limit.
    pub(crate) high_key: Key,
    /// Notes outside these limits are transposed to the nearest octave within the range.
    /// [`note_range.0`] Note lower limit
    /// [`note_range.1`] Note higher limit
    pub(crate) note_range: (u7, u7),
    pub(crate) retrigger_rule: RetriggerRule,
}

impl<'a> TryFrom<(&'a [u8], Version)> for Table {
    type Error = Error;

    fn try_from(value: (&'a [u8], Version)) -> Result<Self> {
        let (value, version) = value;
        if value.len() < TABLE_SIZE {
            bail!(err_malformed!("data field too small"));
        }

        let ntr = TranspositionType::try_from((value[0], version))?;
        let ntt = TranspositionTable::try_from((value[1], version))?;
        let bass_on = (value[1] & 0b1000_0000 != 0) && version == Version::Ctab2;
        let high_key = Key::try_from(value[2])?;
        let note_range_low = u7::from(value[3]);
        let note_range_high = u7::from(value[4]);
        let retrigger_rule = RetriggerRule::try_from(value[5])?;

        Ok(Table { ntr, ntt, bass_on, high_key, note_range: (note_range_low, note_range_high), retrigger_rule, })
    }
}

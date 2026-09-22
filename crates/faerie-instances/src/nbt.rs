//! Just enough NBT to read and write `servers.dat` without losing anything
//! the game put there: every tag type, big-endian, uncompressed (which is
//! how Minecraft stores that file). Strings are kept as raw bytes so the
//! game's modified UTF-8 round-trips untouched.

use std::fmt;

#[derive(Debug, Clone, PartialEq)]
pub enum Tag {
    Byte(i8),
    Short(i16),
    Int(i32),
    Long(i64),
    Float(f32),
    Double(f64),
    ByteArray(Vec<u8>),
    String(Vec<u8>),
    /// Element type id and the elements. The id matters only for empty lists
    /// (the game writes `TAG_End` there); otherwise it is derived on write.
    List(u8, Vec<Tag>),
    /// Ordered, so the file is rewritten the way it was read.
    Compound(Vec<(Vec<u8>, Tag)>),
    IntArray(Vec<i32>),
    LongArray(Vec<i64>),
}

#[derive(Debug)]
pub struct NbtError(pub String);

impl fmt::Display for NbtError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl std::error::Error for NbtError {}

impl Tag {
    fn id(&self) -> u8 {
        match self {
            Tag::Byte(_) => 1,
            Tag::Short(_) => 2,
            Tag::Int(_) => 3,
            Tag::Long(_) => 4,
            Tag::Float(_) => 5,
            Tag::Double(_) => 6,
            Tag::ByteArray(_) => 7,
            Tag::String(_) => 8,
            Tag::List(..) => 9,
            Tag::Compound(_) => 10,
            Tag::IntArray(_) => 11,
            Tag::LongArray(_) => 12,
        }
    }

    /// Look up a child of a compound by name.
    pub fn get(&self, name: &str) -> Option<&Tag> {
        match self {
            Tag::Compound(entries) => entries
                .iter()
                .find(|(k, _)| k == name.as_bytes())
                .map(|(_, v)| v),
            _ => None,
        }
    }

    pub fn get_mut(&mut self, name: &str) -> Option<&mut Tag> {
        match self {
            Tag::Compound(entries) => entries
                .iter_mut()
                .find(|(k, _)| k == name.as_bytes())
                .map(|(_, v)| v),
            _ => None,
        }
    }

    /// Set (replace or append) a child of a compound.
    pub fn set(&mut self, name: &str, value: Tag) {
        if let Tag::Compound(entries) = self {
            match entries.iter_mut().find(|(k, _)| k == name.as_bytes()) {
                Some(slot) => slot.1 = value,
                None => entries.push((name.as_bytes().to_vec(), value)),
            }
        }
    }

    pub fn as_str_bytes(&self) -> Option<&[u8]> {
        match self {
            Tag::String(bytes) => Some(bytes),
            _ => None,
        }
    }
}

/// Parse a root tag: `(name, tag)`. `servers.dat` has an unnamed compound.
pub fn read_root(bytes: &[u8]) -> Result<(Vec<u8>, Tag), NbtError> {
    let mut reader = Reader { bytes, pos: 0 };
    let id = reader.u8()?;
    if id == 0 {
        return Err(NbtError("root tag is TAG_End".into()));
    }
    let name = reader.string()?;
    let tag = reader.payload(id)?;
    if reader.pos != bytes.len() {
        return Err(NbtError(format!(
            "{} trailing byte(s) after the root tag",
            bytes.len() - reader.pos
        )));
    }
    Ok((name, tag))
}

pub fn write_root(name: &[u8], tag: &Tag) -> Vec<u8> {
    let mut out = Vec::new();
    out.push(tag.id());
    write_string(&mut out, name);
    write_payload(&mut out, tag);
    out
}

struct Reader<'a> {
    bytes: &'a [u8],
    pos: usize,
}

impl Reader<'_> {
    fn take(&mut self, n: usize) -> Result<&[u8], NbtError> {
        let end = self
            .pos
            .checked_add(n)
            .ok_or_else(|| NbtError("length overflow".into()))?;
        let slice = self
            .bytes
            .get(self.pos..end)
            .ok_or_else(|| NbtError(format!("unexpected end of data at byte {}", self.pos)))?;
        self.pos = end;
        Ok(slice)
    }

    fn u8(&mut self) -> Result<u8, NbtError> {
        Ok(self.take(1)?[0])
    }

    fn i16(&mut self) -> Result<i16, NbtError> {
        Ok(i16::from_be_bytes(self.take(2)?.try_into().unwrap()))
    }

    fn i32(&mut self) -> Result<i32, NbtError> {
        Ok(i32::from_be_bytes(self.take(4)?.try_into().unwrap()))
    }

    fn i64(&mut self) -> Result<i64, NbtError> {
        Ok(i64::from_be_bytes(self.take(8)?.try_into().unwrap()))
    }

    fn len(&mut self) -> Result<usize, NbtError> {
        let n = self.i32()?;
        usize::try_from(n).map_err(|_| NbtError(format!("negative length {n}")))
    }

    fn string(&mut self) -> Result<Vec<u8>, NbtError> {
        let n = self.i16()? as u16 as usize;
        Ok(self.take(n)?.to_vec())
    }

    fn payload(&mut self, id: u8) -> Result<Tag, NbtError> {
        Ok(match id {
            1 => Tag::Byte(self.u8()? as i8),
            2 => Tag::Short(self.i16()?),
            3 => Tag::Int(self.i32()?),
            4 => Tag::Long(self.i64()?),
            5 => Tag::Float(f32::from_bits(self.i32()? as u32)),
            6 => Tag::Double(f64::from_bits(self.i64()? as u64)),
            7 => {
                let n = self.len()?;
                Tag::ByteArray(self.take(n)?.to_vec())
            }
            8 => Tag::String(self.string()?),
            9 => {
                let elem = self.u8()?;
                let n = self.len()?;
                let mut items = Vec::with_capacity(n.min(1 << 16));
                for _ in 0..n {
                    items.push(self.payload(elem)?);
                }
                Tag::List(elem, items)
            }
            10 => {
                let mut entries = Vec::new();
                loop {
                    let child = self.u8()?;
                    if child == 0 {
                        break;
                    }
                    let name = self.string()?;
                    entries.push((name, self.payload(child)?));
                }
                Tag::Compound(entries)
            }
            11 => {
                let n = self.len()?;
                let mut items = Vec::with_capacity(n.min(1 << 16));
                for _ in 0..n {
                    items.push(self.i32()?);
                }
                Tag::IntArray(items)
            }
            12 => {
                let n = self.len()?;
                let mut items = Vec::with_capacity(n.min(1 << 16));
                for _ in 0..n {
                    items.push(self.i64()?);
                }
                Tag::LongArray(items)
            }
            other => return Err(NbtError(format!("unknown tag type {other}"))),
        })
    }
}

fn write_string(out: &mut Vec<u8>, s: &[u8]) {
    let n = u16::try_from(s.len()).unwrap_or(u16::MAX) as usize;
    out.extend_from_slice(&(n as u16).to_be_bytes());
    out.extend_from_slice(&s[..n]);
}

fn write_payload(out: &mut Vec<u8>, tag: &Tag) {
    match tag {
        Tag::Byte(v) => out.push(*v as u8),
        Tag::Short(v) => out.extend_from_slice(&v.to_be_bytes()),
        Tag::Int(v) => out.extend_from_slice(&v.to_be_bytes()),
        Tag::Long(v) => out.extend_from_slice(&v.to_be_bytes()),
        Tag::Float(v) => out.extend_from_slice(&v.to_bits().to_be_bytes()),
        Tag::Double(v) => out.extend_from_slice(&v.to_bits().to_be_bytes()),
        Tag::ByteArray(v) => {
            out.extend_from_slice(&(v.len() as i32).to_be_bytes());
            out.extend_from_slice(v);
        }
        Tag::String(s) => write_string(out, s),
        Tag::List(elem, items) => {
            let elem = items.first().map(Tag::id).unwrap_or(*elem);
            out.push(elem);
            out.extend_from_slice(&(items.len() as i32).to_be_bytes());
            for item in items {
                write_payload(out, item);
            }
        }
        Tag::Compound(entries) => {
            for (name, value) in entries {
                out.push(value.id());
                write_string(out, name);
                write_payload(out, value);
            }
            out.push(0);
        }
        Tag::IntArray(v) => {
            out.extend_from_slice(&(v.len() as i32).to_be_bytes());
            for i in v {
                out.extend_from_slice(&i.to_be_bytes());
            }
        }
        Tag::LongArray(v) => {
            out.extend_from_slice(&(v.len() as i32).to_be_bytes());
            for i in v {
                out.extend_from_slice(&i.to_be_bytes());
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The 19 bytes Minecraft writes for an empty server list.
    const EMPTY_SERVERS: &[u8] = &[
        0x0a, 0x00, 0x00, // compound ""
        0x09, 0x00, 0x07, b's', b'e', b'r', b'v', b'e', b'r', b's', // list "servers"
        0x00, 0x00, 0x00, 0x00, 0x00, // TAG_End elements, 0 of them
        0x00, // end of root
    ];

    #[test]
    fn reads_and_rewrites_the_games_empty_list_byte_for_byte() {
        let (name, root) = read_root(EMPTY_SERVERS).unwrap();
        assert!(name.is_empty());
        assert_eq!(root.get("servers"), Some(&Tag::List(0, vec![])));
        assert_eq!(write_root(&name, &root), EMPTY_SERVERS);
    }

    #[test]
    fn every_tag_type_round_trips() {
        let root = Tag::Compound(vec![
            (b"b".to_vec(), Tag::Byte(-3)),
            (b"s".to_vec(), Tag::Short(-300)),
            (b"i".to_vec(), Tag::Int(70_000)),
            (b"l".to_vec(), Tag::Long(-(1 << 40))),
            (b"f".to_vec(), Tag::Float(1.5)),
            (b"d".to_vec(), Tag::Double(-2.25)),
            (b"ba".to_vec(), Tag::ByteArray(vec![1, 2, 3])),
            (
                b"str".to_vec(),
                Tag::String("F\u{29A}\u{29E}".as_bytes().to_vec()),
            ),
            (
                b"list".to_vec(),
                Tag::List(10, vec![Tag::Compound(vec![(b"x".to_vec(), Tag::Int(1))])]),
            ),
            (b"ia".to_vec(), Tag::IntArray(vec![-1, 2])),
            (b"la".to_vec(), Tag::LongArray(vec![3, -4])),
        ]);
        let bytes = write_root(b"", &root);
        let (_, back) = read_root(&bytes).unwrap();
        assert_eq!(back, root);
    }

    #[test]
    fn truncated_and_trailing_data_are_errors() {
        assert!(read_root(&EMPTY_SERVERS[..10]).is_err());
        let mut extra = EMPTY_SERVERS.to_vec();
        extra.push(0x2a);
        assert!(read_root(&extra).is_err());
    }
}

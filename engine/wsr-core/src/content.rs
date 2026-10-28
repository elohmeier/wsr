//! Immutable runtime resources. Only identifiers and extraction metadata are shipped.
use crate::pb::pbstr::PbStr;
use std::{cell::RefCell, collections::BTreeMap, fmt, sync::Arc};

mod schema {
    include!("content_ids.rs");
}
pub use schema::REQUIRED_IDS;
const MAGIC: &[u8; 8] = b"WSRCP002";
const MAX_TEXT_BYTES: usize = 65536;
const MAX_PACK_BYTES: usize = 32 * 1024 * 1024;
pub const CONTENT_FILE: &str = "ENGINE-CONTENT.WSR";
pub const REMARK_COUNT: usize = 27;

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct TextId(u32);
impl TextId {
    pub const STARTUP_TICKER: Self = Self(1);
    pub fn bankruptcy_remark(index: usize) -> Option<Self> {
        (index < REMARK_COUNT).then(|| Self(100 + index as u32))
    }
    pub fn identifiers() -> impl Iterator<Item = Self> {
        REQUIRED_IDS.iter().copied().map(Self)
    }
    pub fn binary_length(self) -> Option<usize> {
        schema::BINARY_LENGTHS
            .binary_search_by_key(&self.0, |&(id, _)| id)
            .ok()
            .map(|i| schema::BINARY_LENGTHS[i].1)
    }
    fn from_wire(id: u32) -> Option<Self> {
        REQUIRED_IDS.binary_search(&id).ok().map(|_| Self(id))
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ContentError(&'static str);
impl fmt::Display for ContentError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.0)
    }
}
impl std::error::Error for ContentError {}

#[derive(Clone, Debug)]
struct Text {
    bytes: PbStr,
    display: String,
}
#[derive(Clone, Debug)]
pub struct ContentPack {
    texts: BTreeMap<TextId, Text>,
}
impl ContentPack {
    /// Hosts may also supply complete, independently authored catalogs.
    pub fn from_entries(
        entries: impl IntoIterator<Item = (TextId, PbStr)>,
    ) -> Result<Self, ContentError> {
        let mut texts = BTreeMap::new();
        for (id, bytes) in entries {
            if bytes.is_empty()
                || bytes.as_bytes().len() > MAX_TEXT_BYTES
                || id
                    .binary_length()
                    .is_some_and(|n| n != bytes.as_bytes().len())
            {
                return Err(ContentError("Content record has an invalid length"));
            }
            let display = bytes.to_display();
            if texts.insert(id, Text { bytes, display }).is_some() {
                return Err(ContentError("Duplicate content identifier"));
            }
        }
        if texts.len() != REQUIRED_IDS.len() {
            return Err(ContentError(
                "Incomplete runtime resources; import the installer again",
            ));
        }
        Ok(Self { texts })
    }
    pub fn parse(bytes: &[u8]) -> Result<Self, ContentError> {
        if bytes.len() < 16 || bytes.len() > MAX_PACK_BYTES {
            return Err(ContentError("Invalid content pack size"));
        }
        let (payload, check) = bytes.split_at(bytes.len() - 4);
        if crc32(payload) != u32::from_le_bytes(check.try_into().unwrap()) {
            return Err(ContentError("Content pack checksum mismatch"));
        }
        let mut rest = payload;
        if take(&mut rest, 8)? != MAGIC {
            return Err(ContentError("Unsupported content pack version"));
        }
        if word(&mut rest)? as usize != REQUIRED_IDS.len() {
            return Err(ContentError("Unexpected content entry count"));
        }
        let mut entries = Vec::with_capacity(REQUIRED_IDS.len());
        for _ in REQUIRED_IDS {
            let id = TextId::from_wire(word(&mut rest)?)
                .ok_or(ContentError("Unknown content identifier"))?;
            let length = word(&mut rest)? as usize;
            if length == 0 || length > MAX_TEXT_BYTES {
                return Err(ContentError("Content record has an invalid length"));
            }
            entries.push((id, PbStr::from(take(&mut rest, length)?)));
        }
        if !rest.is_empty() {
            return Err(ContentError("Trailing content pack bytes"));
        }
        Self::from_entries(entries)
    }
    pub fn text(&self, id: TextId) -> &PbStr {
        &self.texts[&id].bytes
    }
    pub fn string(&self, id: u32) -> &str {
        &self.texts[&TextId(id)].display
    }
    pub fn bytes(&self, id: u32) -> &[u8] {
        self.texts[&TextId(id)].bytes.as_bytes()
    }
    pub fn single(&self, id: u32) -> f32 {
        f32::from_le_bytes(self.bytes(id).try_into().expect("Invalid SINGLE resource"))
    }
    pub fn double(&self, id: u32) -> f64 {
        f64::from_le_bytes(self.bytes(id).try_into().expect("Invalid DOUBLE resource"))
    }
    pub fn long(&self, id: u32) -> i32 {
        i32::from_le_bytes(self.bytes(id).try_into().expect("Invalid LONG resource"))
    }
    pub fn to_bytes(&self) -> Vec<u8> {
        let mut out = MAGIC.to_vec();
        out.extend_from_slice(&(REQUIRED_IDS.len() as u32).to_le_bytes());
        for (id, text) in &self.texts {
            out.extend_from_slice(&id.0.to_le_bytes());
            out.extend_from_slice(&(text.bytes.as_bytes().len() as u32).to_le_bytes());
            out.extend_from_slice(text.bytes.as_bytes());
        }
        out.extend_from_slice(&crc32(&out).to_le_bytes());
        out
    }
}

thread_local! { static CURRENT: RefCell<Option<Arc<ContentPack>>> = const { RefCell::new(None) }; }
/// One catalog per existing engine host thread / WASM instance. Function scopes retain an Arc
/// while borrowing strings, so replacing a catalog cannot invalidate in-flight references.
pub fn install(content: Arc<ContentPack>) {
    CURRENT.with(|slot| *slot.borrow_mut() = Some(content));
}
pub fn current_optional() -> Option<Arc<ContentPack>> {
    CURRENT.with(|slot| slot.borrow().clone())
}
pub fn current() -> Arc<ContentPack> {
    if let Some(content) = current_optional() {
        return content;
    }
    #[cfg(not(target_arch = "wasm32"))]
    if let Some(path) = std::env::var_os("WSR_CONTENT_PACK") {
        let bytes = std::fs::read(path).expect("Unable to read WSR_CONTENT_PACK");
        let content = Arc::new(ContentPack::parse(&bytes).expect("Invalid WSR_CONTENT_PACK"));
        install(content.clone());
        return content;
    }
    panic!("Runtime resources are unavailable; import the installer or set WSR_CONTENT_PACK")
}
fn take<'a>(rest: &mut &'a [u8], length: usize) -> Result<&'a [u8], ContentError> {
    if length > rest.len() {
        return Err(ContentError("Truncated content pack"));
    }
    let (head, tail) = rest.split_at(length);
    *rest = tail;
    Ok(head)
}
fn word(rest: &mut &[u8]) -> Result<u32, ContentError> {
    Ok(u32::from_le_bytes(take(rest, 4)?.try_into().unwrap()))
}
fn crc32(bytes: &[u8]) -> u32 {
    let mut crc = !0u32;
    for &byte in bytes {
        crc ^= u32::from(byte);
        for _ in 0..8 {
            crc = (crc >> 1) ^ (0xedb88320 & 0u32.wrapping_sub(crc & 1));
        }
    }
    !crc
}

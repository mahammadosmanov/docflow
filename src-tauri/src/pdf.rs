use std::{
    collections::{BTreeMap, BTreeSet, HashMap, HashSet},
    fmt, fs,
    io::{Read, Write},
    path::Path,
    sync::Arc,
};

use flate2::read::ZlibDecoder;
use sha2::{Digest, Sha256};

pub(crate) const MAX_PDF_BYTES: u64 = 512 * 1024 * 1024;
const MAX_OBJECTS: usize = 1_000_000;
const MAX_DEPTH: usize = 128;
const MAX_DECOMPRESSED_STREAM: usize = 256 * 1024 * 1024;

type ObjectId = (u32, u16);
type Dictionary = BTreeMap<Vec<u8>, PdfObject>;

#[derive(Clone, Debug, PartialEq)]
enum PdfObject {
    Null,
    Bool(bool),
    Number(String),
    Name(Vec<u8>),
    String(Vec<u8>),
    Array(Vec<PdfObject>),
    Dictionary(Dictionary),
    Reference(ObjectId),
    Stream {
        dictionary: Dictionary,
        data: Vec<u8>,
    },
}

impl PdfObject {
    fn as_i64(&self) -> Option<i64> {
        match self {
            Self::Number(value) => value.parse().ok(),
            _ => None,
        }
    }

    fn as_usize(&self) -> Option<usize> {
        self.as_i64().and_then(|value| usize::try_from(value).ok())
    }

    fn as_name(&self) -> Option<&[u8]> {
        match self {
            Self::Name(value) => Some(value),
            _ => None,
        }
    }

    fn as_reference(&self) -> Option<ObjectId> {
        match self {
            Self::Reference(id) => Some(*id),
            _ => None,
        }
    }

    fn as_array(&self) -> Option<&[Self]> {
        match self {
            Self::Array(values) => Some(values),
            _ => None,
        }
    }

    fn as_dictionary(&self) -> Option<&Dictionary> {
        match self {
            Self::Dictionary(value)
            | Self::Stream {
                dictionary: value, ..
            } => Some(value),
            _ => None,
        }
    }
}

fn contains_reference(object: &PdfObject) -> bool {
    match object {
        PdfObject::Reference(_) => true,
        PdfObject::Array(values) => values.iter().any(contains_reference),
        PdfObject::Dictionary(values)
        | PdfObject::Stream {
            dictionary: values, ..
        } => values.values().any(contains_reference),
        _ => false,
    }
}

#[derive(Clone, Debug)]
enum XrefEntry {
    Offset { offset: usize, generation: u16 },
    Compressed { stream: u32, index: usize },
}

#[derive(Clone, Debug)]
struct ParsedPdf {
    bytes: Arc<[u8]>,
    entries: BTreeMap<u32, XrefEntry>,
    trailers: Vec<Dictionary>,
    last_xref: usize,
    revisions: usize,
    max_id: u32,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PdfPageInspection {
    pub object_id: (u32, u16),
    pub object_hash: String,
    pub media_box: Vec<String>,
    pub crop_box: Vec<String>,
    pub rotation: i64,
    pub content_hash: String,
    pub annotations: usize,
    pub links: usize,
    pub widgets: usize,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PdfInspection {
    pub page_count: usize,
    pub pages: Vec<PdfPageInspection>,
    pub catalog_entry_hashes: BTreeMap<String, String>,
    pub metadata_xml_self_contained: bool,
    pub empty_outline_tree: bool,
    pub info_hash: Option<String>,
    pub embedded_file_hashes: BTreeMap<String, String>,
    pub incremental_revisions: usize,
    pub object_stream_count: usize,
    pub encrypted: bool,
    pub signed: bool,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PdfError {
    message: String,
}

impl PdfError {
    fn new(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
        }
    }

    pub const fn reviewable(&self) -> bool {
        true
    }
}

impl fmt::Display for PdfError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.message)
    }
}

impl std::error::Error for PdfError {}

mod codec;
mod inspect;
mod parsed;
mod parser;
mod serialize;
mod validate;
use codec::*;
pub use inspect::*;
use parser::*;
use serialize::*;
pub use validate::*;

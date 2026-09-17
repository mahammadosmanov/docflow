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

pub fn inspect(path: &Path) -> Result<PdfInspection, PdfError> {
    ParsedPdf::load(path)?.inspect()
}

pub fn replace_first_page_incremental(
    archive_path: &Path,
    signed_path: &Path,
    output_path: &Path,
) -> Result<(), PdfError> {
    replace_first_page_incremental_from_page(archive_path, signed_path, 0, output_path)
}

pub fn replace_first_page_incremental_from_page(
    archive_path: &Path,
    signed_path: &Path,
    signed_page_index: usize,
    output_path: &Path,
) -> Result<(), PdfError> {
    if output_path.exists() {
        return Err(PdfError::new(format!(
            "review destination already exists: {}",
            output_path.display()
        )));
    }
    let archive = ParsedPdf::load(archive_path)?;
    let signed = ParsedPdf::load(signed_path)?;
    let archive_facts = archive.inspect()?;
    let signed_facts = signed.inspect()?;
    reject_unsupported(&archive_facts, "archive")?;
    reject_unsupported(&signed_facts, "signed scan")?;
    if archive_facts.page_count == 0 || signed_page_index >= signed_facts.page_count {
        return Err(PdfError::new("PDF has no pages"));
    }
    if archive_facts.pages[0].widgets > 0 {
        return Err(PdfError::new(
            "archive page one contains a form widget that cannot be safely detached",
        ));
    }
    if signed_facts.pages[signed_page_index].widgets > 0 {
        return Err(PdfError::new(
            "signed page one contains a form widget without a safely importable field tree",
        ));
    }

    let appended = build_incremental_replacement(&archive, &signed, signed_page_index)?;
    let parent = output_path
        .parent()
        .ok_or_else(|| PdfError::new("review destination has no parent directory"))?;
    if !parent.is_dir() {
        return Err(PdfError::new("review destination parent does not exist"));
    }
    let mut output = fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(output_path)
        .map_err(|error| PdfError::new(format!("review output create failed: {error}")))?;
    let write_result = (|| -> Result<(), PdfError> {
        output
            .write_all(&archive.bytes)
            .and_then(|()| output.write_all(&appended))
            .and_then(|()| output.sync_all())
            .map_err(|error| PdfError::new(format!("review output write failed: {error}")))?;
        drop(output);
        validate_completed(
            output_path,
            &archive.bytes,
            &archive_facts,
            &signed_facts,
            signed_page_index,
        )
    })();
    if write_result.is_err() {
        let _ = fs::remove_file(output_path);
    }
    write_result
}

/// Writes a new PDF by appending page-dictionary revisions that change only
/// the approved display rotations. The source bytes remain an exact prefix of
/// the result so unrelated PDF objects are preserved byte-for-byte.
pub fn rotate_pages_incremental(
    source_path: &Path,
    output_path: &Path,
    rotations: &[(usize, i64)],
) -> Result<(), PdfError> {
    if output_path.exists() {
        return Err(PdfError::new(format!(
            "review destination already exists: {}",
            output_path.display()
        )));
    }
    if rotations.is_empty() {
        return Err(PdfError::new("no page rotations were approved"));
    }
    let source = ParsedPdf::load(source_path)?;
    let before = source.inspect()?;
    if before.encrypted {
        return Err(PdfError::new(
            "source PDF is encrypted; orientation repair is intentionally disabled",
        ));
    }
    if before.signed {
        return Err(PdfError::new(
            "source PDF contains a digital signature; orientation repair would invalidate it",
        ));
    }
    let mut approved = BTreeMap::new();
    for &(page_index, rotation) in rotations {
        if page_index >= before.page_count {
            return Err(PdfError::new("approved page index is out of range"));
        }
        if !matches!(rotation, 0 | 90 | 180 | 270) {
            return Err(PdfError::new(
                "approved page rotation must be 0, 90, 180, or 270 degrees",
            ));
        }
        if approved.insert(page_index, rotation).is_some() {
            return Err(PdfError::new("approved page index is duplicated"));
        }
    }
    let appended = build_incremental_rotations(&source, &approved)?;
    let parent = output_path
        .parent()
        .ok_or_else(|| PdfError::new("review destination has no parent directory"))?;
    if !parent.is_dir() {
        return Err(PdfError::new("review destination parent does not exist"));
    }
    let mut output = fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(output_path)
        .map_err(|error| PdfError::new(format!("review output create failed: {error}")))?;
    let write_result = (|| -> Result<(), PdfError> {
        output
            .write_all(&source.bytes)
            .and_then(|()| output.write_all(&appended))
            .and_then(|()| output.sync_all())
            .map_err(|error| PdfError::new(format!("review output write failed: {error}")))?;
        drop(output);
        validate_rotated(output_path, &source.bytes, &before, &approved)
    })();
    if write_result.is_err() {
        let _ = fs::remove_file(output_path);
    }
    write_result
}

fn build_incremental_rotations(
    source: &ParsedPdf,
    rotations: &BTreeMap<usize, i64>,
) -> Result<Vec<u8>, PdfError> {
    let pages = source.pages()?;
    let mut writes = BTreeMap::<ObjectId, PdfObject>::new();
    for (&page_index, &rotation) in rotations {
        let page_id = *pages
            .get(page_index)
            .ok_or_else(|| PdfError::new("approved page index is out of range"))?;
        let mut dictionary = source
            .get_object(page_id)?
            .as_dictionary()
            .cloned()
            .ok_or_else(|| PdfError::new("PDF page is not a dictionary"))?;
        dictionary.insert(b"Rotate".to_vec(), PdfObject::Number(rotation.to_string()));
        writes.insert(page_id, PdfObject::Dictionary(dictionary));
    }
    build_incremental_appendix(source, &writes)
}

fn build_incremental_appendix(
    source: &ParsedPdf,
    writes: &BTreeMap<ObjectId, PdfObject>,
) -> Result<Vec<u8>, PdfError> {
    if writes.is_empty() || writes.len() > MAX_OBJECTS {
        return Err(PdfError::new("PDF incremental write set is invalid"));
    }
    let base_offset = source.bytes.len() + usize::from(!source.bytes.ends_with(b"\n"));
    let mut appendix = Vec::new();
    if !source.bytes.ends_with(b"\n") {
        appendix.push(b'\n');
    }
    let mut offsets = BTreeMap::<ObjectId, usize>::new();
    for (id, object) in writes {
        offsets.insert(*id, base_offset + appendix.len());
        appendix.extend_from_slice(format!("{} {} obj\n", id.0, id.1).as_bytes());
        serialize_object(object, &mut appendix, 0)?;
        appendix.extend_from_slice(b"\nendobj\n");
    }
    let xref_offset = base_offset + appendix.len();
    appendix.extend_from_slice(b"xref\n");
    for ((number, generation), offset) in &offsets {
        appendix
            .extend_from_slice(format!("{number} 1\n{offset:010} {generation:05} n \n").as_bytes());
    }
    let latest = source
        .trailers
        .first()
        .ok_or_else(|| PdfError::new("source PDF has no trailer"))?;
    let mut trailer = latest.clone();
    for key in [
        b"Size".as_slice(),
        b"Prev",
        b"XRefStm",
        b"Encrypt",
        b"Type",
        b"W",
        b"Index",
        b"Length",
        b"Filter",
        b"DecodeParms",
    ] {
        trailer.remove(key);
    }
    trailer.insert(
        b"Size".to_vec(),
        PdfObject::Number((source.max_id + 1).to_string()),
    );
    trailer.insert(
        b"Prev".to_vec(),
        PdfObject::Number(source.last_xref.to_string()),
    );
    appendix.extend_from_slice(b"trailer\n");
    serialize_object(&PdfObject::Dictionary(trailer), &mut appendix, 0)?;
    appendix.extend_from_slice(format!("\nstartxref\n{xref_offset}\n%%EOF\n").as_bytes());
    Ok(appendix)
}

fn validate_rotated(
    output_path: &Path,
    source_bytes: &[u8],
    before: &PdfInspection,
    rotations: &BTreeMap<usize, i64>,
) -> Result<(), PdfError> {
    let output_bytes = read_bounded(output_path)?;
    if !output_bytes.starts_with(source_bytes) {
        return Err(PdfError::new(
            "rotated PDF did not preserve the source revision bytes",
        ));
    }
    let after = ParsedPdf::parse(output_bytes)?.inspect()?;
    if after.page_count != before.page_count
        || after.incremental_revisions != before.incremental_revisions + 1
        || after.object_stream_count != before.object_stream_count
        || after.catalog_entry_hashes != before.catalog_entry_hashes
        || after.info_hash != before.info_hash
        || after.embedded_file_hashes != before.embedded_file_hashes
        || after.encrypted != before.encrypted
        || after.signed != before.signed
    {
        return Err(PdfError::new(
            "rotated PDF failed document-structure fidelity validation",
        ));
    }
    for (index, (old, new)) in before.pages.iter().zip(&after.pages).enumerate() {
        let expected_rotation = rotations.get(&index).copied().unwrap_or(old.rotation);
        if new.object_id != old.object_id
            || new.media_box != old.media_box
            || new.crop_box != old.crop_box
            || new.rotation != expected_rotation
            || new.content_hash != old.content_hash
            || new.annotations != old.annotations
            || new.links != old.links
            || new.widgets != old.widgets
            || (!rotations.contains_key(&index) && new.object_hash != old.object_hash)
        {
            return Err(PdfError::new("rotated PDF failed page-fidelity validation"));
        }
    }
    Ok(())
}

fn reject_unsupported(facts: &PdfInspection, label: &str) -> Result<(), PdfError> {
    if facts.encrypted {
        return Err(PdfError::new(format!(
            "{label} PDF is encrypted; staging is intentionally disabled"
        )));
    }
    if facts.signed {
        return Err(PdfError::new(format!(
            "{label} PDF contains a digital signature; staging is intentionally disabled"
        )));
    }
    if facts.catalog_entry_hashes.contains_key("StructTreeRoot") {
        return Err(PdfError::new(format!(
            "{label} PDF is tagged; staging cannot safely preserve the page structure tree"
        )));
    }
    if label == "signed scan" && facts.catalog_entry_hashes.contains_key("OCProperties") {
        return Err(PdfError::new(
            "signed scan uses document-level optional-content state that cannot be imported safely",
        ));
    }
    Ok(())
}

fn validate_completed(
    output_path: &Path,
    archive_bytes: &[u8],
    archive: &PdfInspection,
    signed: &PdfInspection,
    signed_page_index: usize,
) -> Result<(), PdfError> {
    let output_bytes = read_bounded(output_path)?;
    if !output_bytes.starts_with(archive_bytes) {
        return Err(PdfError::new(
            "completed PDF did not preserve the archive revision bytes",
        ));
    }
    let completed = ParsedPdf::parse(output_bytes)?.inspect()?;
    if completed.encrypted || completed.signed {
        return Err(PdfError::new(
            "completed PDF unexpectedly changed its encryption/signature policy",
        ));
    }
    if completed.page_count != archive.page_count
        || completed.incremental_revisions != archive.incremental_revisions + 1
        || completed.object_stream_count != archive.object_stream_count
        || completed.catalog_entry_hashes != archive.catalog_entry_hashes
        || completed.info_hash != archive.info_hash
        || completed.embedded_file_hashes != archive.embedded_file_hashes
        || completed.pages.get(1..) != archive.pages.get(1..)
    {
        return Err(PdfError::new(
            "completed PDF failed archive-structure fidelity validation",
        ));
    }
    let completed_first = completed
        .pages
        .first()
        .ok_or_else(|| PdfError::new("completed PDF has no first page"))?;
    let signed_selected = signed
        .pages
        .get(signed_page_index)
        .ok_or_else(|| PdfError::new("signed PDF page index is out of range"))?;
    if completed_first.media_box != signed_selected.media_box
        || completed_first.crop_box != signed_selected.crop_box
        || completed_first.rotation != signed_selected.rotation
        || completed_first.content_hash != signed_selected.content_hash
        || completed_first.annotations != signed_selected.annotations
        || completed_first.links != signed_selected.links
        || completed_first.widgets != signed_selected.widgets
    {
        return Err(PdfError::new(
            "completed PDF failed signed first-page validation",
        ));
    }
    Ok(())
}

fn build_incremental_replacement(
    archive: &ParsedPdf,
    signed: &ParsedPdf,
    signed_page_index: usize,
) -> Result<Vec<u8>, PdfError> {
    let archive_pages = archive.pages()?;
    let signed_pages = signed.pages()?;
    let archive_page = *archive_pages
        .first()
        .ok_or_else(|| PdfError::new("archive PDF has no pages"))?;
    let signed_page = *signed_pages
        .get(signed_page_index)
        .ok_or_else(|| PdfError::new("signed PDF page index is out of range"))?;
    let archive_parent = archive
        .get_object(archive_page)?
        .as_dictionary()
        .and_then(|dictionary| dictionary.get(b"Parent".as_slice()))
        .and_then(PdfObject::as_reference)
        .ok_or_else(|| PdfError::new("archive page one has no valid parent"))?;

    let mut page_dictionary = signed
        .get_object(signed_page)?
        .as_dictionary()
        .cloned()
        .ok_or_else(|| PdfError::new("signed page one is not a page dictionary"))?;
    materialize_inherited_page_values(signed, signed_page, &mut page_dictionary)?;
    page_dictionary.remove(b"Parent".as_slice());

    let signed_page_ids = signed_pages.iter().copied().collect::<HashSet<_>>();
    let mut importer = Importer {
        source: signed,
        source_first_page: signed_page,
        target_first_page: archive_page,
        source_page_ids: signed_page_ids,
        next_id: archive.max_id.saturating_add(1),
        assigned: HashMap::new(),
        objects: BTreeMap::new(),
        visiting: HashSet::new(),
    };
    let imported_page = importer.import_object(PdfObject::Dictionary(page_dictionary), 0)?;
    let mut imported_page = match imported_page {
        PdfObject::Dictionary(dictionary) => dictionary,
        _ => return Err(PdfError::new("signed page import changed object type")),
    };
    imported_page.insert(b"Parent".to_vec(), PdfObject::Reference(archive_parent));

    let mut writes = BTreeMap::<ObjectId, PdfObject>::new();
    writes.insert(archive_page, PdfObject::Dictionary(imported_page));
    writes.extend(importer.objects);
    if writes.len() > MAX_OBJECTS {
        return Err(PdfError::new("signed page object graph is too large"));
    }

    let base_offset = archive.bytes.len() + usize::from(!archive.bytes.ends_with(b"\n"));
    let mut appendix = Vec::new();
    if !archive.bytes.ends_with(b"\n") {
        appendix.push(b'\n');
    }
    let mut offsets = BTreeMap::<ObjectId, usize>::new();
    for (id, object) in &writes {
        offsets.insert(*id, base_offset + appendix.len());
        appendix.extend_from_slice(format!("{} {} obj\n", id.0, id.1).as_bytes());
        serialize_object(object, &mut appendix, 0)?;
        appendix.extend_from_slice(b"\nendobj\n");
    }
    let xref_offset = base_offset + appendix.len();
    appendix.extend_from_slice(b"xref\n");
    for ((number, generation), offset) in &offsets {
        appendix
            .extend_from_slice(format!("{number} 1\n{offset:010} {generation:05} n \n").as_bytes());
    }

    let latest = archive
        .trailers
        .first()
        .ok_or_else(|| PdfError::new("archive PDF has no trailer"))?;
    let mut trailer = latest.clone();
    for key in [
        b"Size".as_slice(),
        b"Prev",
        b"XRefStm",
        b"Encrypt",
        b"Type",
        b"W",
        b"Index",
        b"Length",
        b"Filter",
        b"DecodeParms",
    ] {
        trailer.remove(key);
    }
    let max_written = writes.keys().map(|id| id.0).max().unwrap_or(archive.max_id);
    trailer.insert(
        b"Size".to_vec(),
        PdfObject::Number((archive.max_id.max(max_written) + 1).to_string()),
    );
    trailer.insert(
        b"Prev".to_vec(),
        PdfObject::Number(archive.last_xref.to_string()),
    );
    appendix.extend_from_slice(b"trailer\n");
    serialize_object(&PdfObject::Dictionary(trailer), &mut appendix, 0)?;
    appendix.extend_from_slice(format!("\nstartxref\n{xref_offset}\n%%EOF\n").as_bytes());
    Ok(appendix)
}

struct Importer<'a> {
    source: &'a ParsedPdf,
    source_first_page: ObjectId,
    target_first_page: ObjectId,
    source_page_ids: HashSet<ObjectId>,
    next_id: u32,
    assigned: HashMap<ObjectId, ObjectId>,
    objects: BTreeMap<ObjectId, PdfObject>,
    visiting: HashSet<ObjectId>,
}

impl Importer<'_> {
    fn import_object(&mut self, object: PdfObject, depth: usize) -> Result<PdfObject, PdfError> {
        if depth > MAX_DEPTH {
            return Err(PdfError::new(
                "signed page object graph is too deeply nested",
            ));
        }
        Ok(match object {
            PdfObject::Reference(id) if id == self.source_first_page => {
                PdfObject::Reference(self.target_first_page)
            }
            PdfObject::Reference(id) if self.source_page_ids.contains(&id) => {
                return Err(PdfError::new(
                    "signed page references another signed-scan page that will not be staged",
                ));
            }
            PdfObject::Reference(id) => {
                if let Some(mapped) = self.assigned.get(&id) {
                    PdfObject::Reference(*mapped)
                } else {
                    if self.assigned.len() >= MAX_OBJECTS {
                        return Err(PdfError::new("signed page object graph is too large"));
                    }
                    let mapped = (self.next_id, 0);
                    self.next_id = self
                        .next_id
                        .checked_add(1)
                        .ok_or_else(|| PdfError::new("PDF object identity overflow"))?;
                    self.assigned.insert(id, mapped);
                    if !self.visiting.insert(id) {
                        return Ok(PdfObject::Reference(mapped));
                    }
                    let source = self.source.get_object(id)?;
                    let imported = self.import_object(source, depth + 1)?;
                    self.objects.insert(mapped, imported);
                    self.visiting.remove(&id);
                    PdfObject::Reference(mapped)
                }
            }
            PdfObject::Array(values) => PdfObject::Array(
                values
                    .into_iter()
                    .map(|value| self.import_object(value, depth + 1))
                    .collect::<Result<_, _>>()?,
            ),
            PdfObject::Dictionary(values) => PdfObject::Dictionary(
                values
                    .into_iter()
                    .map(|(key, value)| Ok((key, self.import_object(value, depth + 1)?)))
                    .collect::<Result<_, PdfError>>()?,
            ),
            PdfObject::Stream { dictionary, data } => PdfObject::Stream {
                dictionary: dictionary
                    .into_iter()
                    .map(|(key, value)| Ok((key, self.import_object(value, depth + 1)?)))
                    .collect::<Result<_, PdfError>>()?,
                data,
            },
            value => value,
        })
    }
}

fn materialize_inherited_page_values(
    pdf: &ParsedPdf,
    page: ObjectId,
    dictionary: &mut Dictionary,
) -> Result<(), PdfError> {
    for key in [b"Resources".as_slice(), b"MediaBox", b"CropBox", b"Rotate"] {
        if !dictionary.contains_key(key)
            && let Some(value) = pdf.inherited_value(page, key)?
        {
            dictionary.insert(key.to_vec(), value);
        }
    }
    if !dictionary.contains_key(b"MediaBox".as_slice()) {
        return Err(PdfError::new("signed page has no media box"));
    }
    Ok(())
}

impl ParsedPdf {
    fn load(path: &Path) -> Result<Self, PdfError> {
        Self::parse(read_bounded(path)?)
    }

    fn parse(bytes: impl Into<Arc<[u8]>>) -> Result<Self, PdfError> {
        let bytes = bytes.into();
        if !bytes.starts_with(b"%PDF-") {
            return Err(PdfError::new("file is not a PDF"));
        }
        let last_xref = find_startxref(&bytes)?;
        let mut entries = BTreeMap::new();
        let mut defined = BTreeSet::new();
        let mut trailers = Vec::new();
        let mut current = Some(last_xref);
        let mut seen = HashSet::new();
        let mut revisions = 0;
        while let Some(offset) = current {
            if offset >= bytes.len() || !seen.insert(offset) || revisions > 1024 {
                return Err(PdfError::new("PDF cross-reference chain is invalid"));
            }
            let section = parse_xref_section(&bytes, offset)?;
            for number in section.defined {
                if defined.insert(number)
                    && let Some(entry) = section.entries.get(&number)
                {
                    entries.insert(number, entry.clone());
                }
            }
            current = section
                .trailer
                .get(b"Prev".as_slice())
                .and_then(PdfObject::as_usize);
            trailers.push(section.trailer);
            revisions += 1;
        }
        if entries.len() > MAX_OBJECTS {
            return Err(PdfError::new("PDF contains too many objects"));
        }
        let max_id = entries
            .keys()
            .copied()
            .chain(
                trailers
                    .iter()
                    .filter_map(|trailer| trailer.get(b"Size".as_slice()))
                    .filter_map(PdfObject::as_usize)
                    .filter_map(|size| u32::try_from(size.saturating_sub(1)).ok()),
            )
            .max()
            .unwrap_or(0);
        Ok(Self {
            bytes,
            entries,
            trailers,
            last_xref,
            revisions,
            max_id,
        })
    }

    fn get_object(&self, id: ObjectId) -> Result<PdfObject, PdfError> {
        self.get_object_resolving_lengths(id, &mut HashSet::new())
    }

    fn get_object_resolving_lengths(
        &self,
        id: ObjectId,
        resolving: &mut HashSet<ObjectId>,
    ) -> Result<PdfObject, PdfError> {
        if resolving.len() > MAX_DEPTH || !resolving.insert(id) {
            return Err(PdfError::new(
                "PDF stream /Length reference is cyclic or too deep",
            ));
        }
        let entry = self
            .entries
            .get(&id.0)
            .ok_or_else(|| PdfError::new(format!("PDF object {} is missing", id.0)))?;
        let result = match entry {
            XrefEntry::Offset { offset, generation } => {
                if *generation != id.1 {
                    return Err(PdfError::new(format!(
                        "PDF object {} generation is inconsistent",
                        id.0
                    )));
                }
                let parsed = parse_indirect_object(&self.bytes, *offset, |length_id| {
                    self.get_object_resolving_lengths(length_id, resolving)?
                        .as_usize()
                        .ok_or_else(|| {
                            PdfError::new(
                                "PDF stream indirect /Length is not a non-negative integer",
                            )
                        })
                })?;
                if parsed.id != id {
                    return Err(PdfError::new(format!(
                        "PDF cross-reference for object {} is inconsistent",
                        id.0
                    )));
                }
                Ok(parsed.object)
            }
            XrefEntry::Compressed { stream, index } => {
                if id.1 != 0 {
                    return Err(PdfError::new("compressed PDF object has a generation"));
                }
                self.get_compressed_object(*stream, *index, id.0, resolving)
            }
        };
        resolving.remove(&id);
        result
    }

    fn get_compressed_object(
        &self,
        stream_id: u32,
        index: usize,
        expected_number: u32,
        resolving: &mut HashSet<ObjectId>,
    ) -> Result<PdfObject, PdfError> {
        let stream = self.get_object_resolving_lengths((stream_id, 0), resolving)?;
        let PdfObject::Stream { dictionary, data } = stream else {
            return Err(PdfError::new("object stream reference is not a stream"));
        };
        if dictionary
            .get(b"Type".as_slice())
            .and_then(PdfObject::as_name)
            != Some(b"ObjStm")
        {
            return Err(PdfError::new("compressed object container is not /ObjStm"));
        }
        let count = dictionary
            .get(b"N".as_slice())
            .and_then(PdfObject::as_usize)
            .ok_or_else(|| PdfError::new("object stream has no valid /N"))?;
        let first = dictionary
            .get(b"First".as_slice())
            .and_then(PdfObject::as_usize)
            .ok_or_else(|| PdfError::new("object stream has no valid /First"))?;
        if index >= count {
            return Err(PdfError::new("compressed object index is invalid"));
        }
        let decoded = decode_stream(&dictionary, &data)?;
        if first > decoded.len() {
            return Err(PdfError::new("object stream header is invalid"));
        }
        let mut parser = Parser::new(&decoded[..first]);
        let mut offsets = Vec::with_capacity(count);
        for _ in 0..count {
            let number = parser.read_unsigned()?;
            let offset = parser.read_unsigned()?;
            offsets.push((
                u32::try_from(number).map_err(|_| PdfError::new("object number overflow"))?,
                offset,
            ));
        }
        if offsets[index].0 != expected_number {
            return Err(PdfError::new("object stream index does not match xref"));
        }
        let start = first
            .checked_add(offsets[index].1)
            .ok_or_else(|| PdfError::new("object stream offset overflow"))?;
        let end = offsets
            .get(index + 1)
            .and_then(|(_, offset)| first.checked_add(*offset))
            .unwrap_or(decoded.len());
        if start > end || end > decoded.len() {
            return Err(PdfError::new("object stream object bounds are invalid"));
        }
        Parser::new(&decoded[start..end]).parse_object(0)
    }

    fn root_id(&self) -> Result<ObjectId, PdfError> {
        self.trailers
            .iter()
            .find_map(|trailer| trailer.get(b"Root".as_slice()))
            .and_then(PdfObject::as_reference)
            .ok_or_else(|| PdfError::new("PDF catalog reference is missing"))
    }

    fn pages(&self) -> Result<Vec<ObjectId>, PdfError> {
        let root = self.get_object(self.root_id()?)?;
        let pages = root
            .as_dictionary()
            .and_then(|dictionary| dictionary.get(b"Pages".as_slice()))
            .and_then(PdfObject::as_reference)
            .ok_or_else(|| PdfError::new("PDF page tree root is missing"))?;
        let mut result = Vec::new();
        let mut visiting = HashSet::new();
        self.walk_pages(pages, &mut result, &mut visiting, 0)?;
        Ok(result)
    }

    fn walk_pages(
        &self,
        id: ObjectId,
        result: &mut Vec<ObjectId>,
        visiting: &mut HashSet<ObjectId>,
        depth: usize,
    ) -> Result<(), PdfError> {
        if depth > MAX_DEPTH || !visiting.insert(id) {
            return Err(PdfError::new("PDF page tree is cyclic or too deep"));
        }
        let object = self.get_object(id)?;
        let dictionary = object
            .as_dictionary()
            .ok_or_else(|| PdfError::new("PDF page tree object is not a dictionary"))?;
        match dictionary
            .get(b"Type".as_slice())
            .and_then(PdfObject::as_name)
        {
            Some(b"Page") => result.push(id),
            Some(b"Pages") | None => {
                let kids = dictionary
                    .get(b"Kids".as_slice())
                    .and_then(PdfObject::as_array)
                    .ok_or_else(|| PdfError::new("PDF page tree node has no /Kids"))?;
                for kid in kids {
                    let kid = kid
                        .as_reference()
                        .ok_or_else(|| PdfError::new("PDF page tree child is not indirect"))?;
                    self.walk_pages(kid, result, visiting, depth + 1)?;
                }
            }
            _ => return Err(PdfError::new("PDF page tree contains a non-page object")),
        }
        visiting.remove(&id);
        Ok(())
    }

    fn inherited_value(&self, page: ObjectId, key: &[u8]) -> Result<Option<PdfObject>, PdfError> {
        let mut current = Some(page);
        let mut seen = HashSet::new();
        while let Some(id) = current {
            if !seen.insert(id) || seen.len() > MAX_DEPTH {
                return Err(PdfError::new("PDF page inheritance chain is invalid"));
            }
            let object = self.get_object(id)?;
            let dictionary = object
                .as_dictionary()
                .ok_or_else(|| PdfError::new("PDF page ancestor is not a dictionary"))?;
            if let Some(value) = dictionary.get(key) {
                return Ok(Some(value.clone()));
            }
            current = dictionary
                .get(b"Parent".as_slice())
                .and_then(PdfObject::as_reference);
        }
        Ok(None)
    }

    fn inspect(&self) -> Result<PdfInspection, PdfError> {
        let encrypted = self
            .trailers
            .iter()
            .any(|trailer| trailer.contains_key(b"Encrypt".as_slice()));
        if encrypted {
            return Ok(PdfInspection {
                page_count: 0,
                pages: Vec::new(),
                catalog_entry_hashes: BTreeMap::new(),
                info_hash: None,
                embedded_file_hashes: BTreeMap::new(),
                incremental_revisions: self.revisions,
                object_stream_count: self.object_stream_count(),
                encrypted: true,
                signed: self.has_signature_shallow(),
            });
        }
        let signed = self.has_signature()?;
        let page_ids = self.pages()?;
        let mut pages = Vec::with_capacity(page_ids.len());
        for id in page_ids {
            pages.push(self.inspect_page(id)?);
        }
        let root = self.get_object(self.root_id()?)?;
        let catalog = root
            .as_dictionary()
            .ok_or_else(|| PdfError::new("PDF catalog is not a dictionary"))?;
        let mut catalog_entry_hashes = BTreeMap::new();
        for key in [
            b"Metadata".as_slice(),
            b"Names",
            b"Outlines",
            b"PageLabels",
            b"AcroForm",
            b"OpenAction",
            b"ViewerPreferences",
            b"Dests",
            b"StructTreeRoot",
            b"OCProperties",
            b"OutputIntents",
        ] {
            if let Some(value) = catalog.get(key) {
                catalog_entry_hashes.insert(
                    String::from_utf8_lossy(key).into_owned(),
                    self.graph_hash(value)?,
                );
            }
        }
        let info_hash = self
            .trailers
            .iter()
            .find_map(|trailer| trailer.get(b"Info".as_slice()))
            .map(|object| self.graph_hash(object))
            .transpose()?;
        Ok(PdfInspection {
            page_count: pages.len(),
            pages,
            catalog_entry_hashes,
            info_hash,
            embedded_file_hashes: self.embedded_files(catalog)?,
            incremental_revisions: self.revisions,
            object_stream_count: self.object_stream_count(),
            encrypted,
            signed,
        })
    }

    fn inspect_page(&self, id: ObjectId) -> Result<PdfPageInspection, PdfError> {
        let object = self.get_object(id)?;
        let dictionary = object
            .as_dictionary()
            .ok_or_else(|| PdfError::new("PDF page is not a dictionary"))?;
        let media_box = self
            .inherited_value(id, b"MediaBox")?
            .and_then(|object| box_values(&object))
            .ok_or_else(|| PdfError::new("PDF page has no valid media box"))?;
        let crop_box = self
            .inherited_value(id, b"CropBox")?
            .and_then(|object| box_values(&object))
            .unwrap_or_else(|| media_box.clone());
        let rotation = self
            .inherited_value(id, b"Rotate")?
            .and_then(|object| object.as_i64())
            .unwrap_or(0);
        let content_hash = match dictionary.get(b"Contents".as_slice()) {
            Some(contents) => self.content_hash(contents)?,
            None => sha256(&[]),
        };
        let annotations = match dictionary.get(b"Annots".as_slice()) {
            Some(value) => self.resolve_array(value)?.to_vec(),
            None => Vec::new(),
        };
        let mut links = 0;
        let mut widgets = 0;
        for annotation in &annotations {
            let annotation = self.resolve(annotation)?;
            let subtype = annotation
                .as_dictionary()
                .and_then(|value| value.get(b"Subtype".as_slice()))
                .and_then(PdfObject::as_name);
            links += usize::from(subtype == Some(b"Link"));
            widgets += usize::from(subtype == Some(b"Widget"));
        }
        Ok(PdfPageInspection {
            object_id: id,
            object_hash: self.object_hash(id)?,
            media_box,
            crop_box,
            rotation,
            content_hash,
            annotations: annotations.len(),
            links,
            widgets,
        })
    }

    fn resolve(&self, object: &PdfObject) -> Result<PdfObject, PdfError> {
        match object {
            PdfObject::Reference(id) => self.get_object(*id),
            value => Ok(value.clone()),
        }
    }

    fn resolve_array(&self, object: &PdfObject) -> Result<Vec<PdfObject>, PdfError> {
        self.resolve(object)?
            .as_array()
            .map(<[PdfObject]>::to_vec)
            .ok_or_else(|| PdfError::new("PDF object is not an array"))
    }

    fn content_hash(&self, object: &PdfObject) -> Result<String, PdfError> {
        let mut digest = Sha256::new();
        for value in self.resolve_array_or_single(object)? {
            let value = self.resolve(&value)?;
            let PdfObject::Stream { data, .. } = value else {
                return Err(PdfError::new("PDF page content is not a stream"));
            };
            digest.update((data.len() as u64).to_be_bytes());
            digest.update(data);
        }
        Ok(format!("{:x}", digest.finalize()))
    }

    fn resolve_array_or_single(&self, object: &PdfObject) -> Result<Vec<PdfObject>, PdfError> {
        let resolved = self.resolve(object)?;
        Ok(match resolved {
            PdfObject::Array(values) => values,
            value => vec![value],
        })
    }

    fn object_hash(&self, id: ObjectId) -> Result<String, PdfError> {
        let mut serialized = Vec::new();
        serialize_object(&self.get_object(id)?, &mut serialized, 0)?;
        Ok(sha256(&serialized))
    }

    fn graph_hash(&self, object: &PdfObject) -> Result<String, PdfError> {
        let mut digest = Sha256::new();
        let mut seen = BTreeSet::new();
        self.hash_graph(object, &mut digest, &mut seen, 0)?;
        Ok(format!("{:x}", digest.finalize()))
    }

    fn hash_graph(
        &self,
        object: &PdfObject,
        digest: &mut Sha256,
        seen: &mut BTreeSet<ObjectId>,
        depth: usize,
    ) -> Result<(), PdfError> {
        if depth > MAX_DEPTH {
            return Err(PdfError::new("PDF object graph is too deeply nested"));
        }
        let mut bytes = Vec::new();
        serialize_object(object, &mut bytes, depth)?;
        digest.update((bytes.len() as u64).to_be_bytes());
        digest.update(bytes);
        match object {
            PdfObject::Reference(id) if seen.insert(*id) => {
                let referenced = self.get_object(*id)?;
                let is_page_tree_object = referenced
                    .as_dictionary()
                    .and_then(|dictionary| dictionary.get(b"Type".as_slice()))
                    .and_then(PdfObject::as_name)
                    .is_some_and(|kind| matches!(kind, b"Page" | b"Pages"));
                if !is_page_tree_object {
                    self.hash_graph(&referenced, digest, seen, depth + 1)?;
                }
            }
            PdfObject::Array(values) => {
                for value in values {
                    self.hash_graph(value, digest, seen, depth + 1)?;
                }
            }
            PdfObject::Dictionary(values)
            | PdfObject::Stream {
                dictionary: values, ..
            } => {
                for value in values.values() {
                    self.hash_graph(value, digest, seen, depth + 1)?;
                }
            }
            _ => {}
        }
        Ok(())
    }

    fn embedded_files(&self, catalog: &Dictionary) -> Result<BTreeMap<String, String>, PdfError> {
        let Some(names) = catalog.get(b"Names".as_slice()) else {
            return Ok(BTreeMap::new());
        };
        let names = self.resolve(names)?;
        let Some(embedded) = names
            .as_dictionary()
            .and_then(|dictionary| dictionary.get(b"EmbeddedFiles".as_slice()))
        else {
            return Ok(BTreeMap::new());
        };
        let mut pairs = Vec::new();
        self.collect_name_tree(embedded, &mut pairs, 0)?;
        let mut result = BTreeMap::new();
        for (name, file_spec) in pairs {
            let file_spec = self.resolve(&file_spec)?;
            let embedded = file_spec
                .as_dictionary()
                .and_then(|dictionary| dictionary.get(b"EF".as_slice()))
                .map(|value| self.resolve(value))
                .transpose()?
                .and_then(|value| value.as_dictionary().cloned())
                .and_then(|dictionary| {
                    dictionary
                        .get(b"UF".as_slice())
                        .or_else(|| dictionary.get(b"F".as_slice()))
                        .cloned()
                });
            let Some(embedded) = embedded else {
                continue;
            };
            let stream = self.resolve(&embedded)?;
            let PdfObject::Stream { data, .. } = stream else {
                return Err(PdfError::new("embedded file is not a stream"));
            };
            result.insert(name, sha256(&data));
        }
        Ok(result)
    }

    fn collect_name_tree(
        &self,
        node: &PdfObject,
        output: &mut Vec<(String, PdfObject)>,
        depth: usize,
    ) -> Result<(), PdfError> {
        if depth > MAX_DEPTH {
            return Err(PdfError::new("PDF name tree is too deeply nested"));
        }
        let node = self.resolve(node)?;
        let dictionary = node
            .as_dictionary()
            .ok_or_else(|| PdfError::new("PDF name tree node is not a dictionary"))?;
        if let Some(names) = dictionary.get(b"Names".as_slice()) {
            let values = self.resolve_array(names)?;
            if !values.len().is_multiple_of(2) {
                return Err(PdfError::new("PDF name tree has an odd entry count"));
            }
            for pair in values.chunks_exact(2) {
                output.push((object_text(&pair[0]), pair[1].clone()));
            }
        }
        if let Some(kids) = dictionary.get(b"Kids".as_slice()) {
            for kid in self.resolve_array(kids)? {
                self.collect_name_tree(&kid, output, depth + 1)?;
            }
        }
        Ok(())
    }

    fn object_stream_count(&self) -> usize {
        self.entries
            .iter()
            .filter_map(|(number, entry)| match entry {
                XrefEntry::Offset { generation, .. } => Some((*number, *generation)),
                XrefEntry::Compressed { .. } => None,
            })
            .filter(|id| {
                self.get_object(*id)
                    .ok()
                    .and_then(|object| {
                        object
                            .as_dictionary()
                            .and_then(|dictionary| dictionary.get(b"Type".as_slice()))
                            .and_then(PdfObject::as_name)
                            .map(<[u8]>::to_vec)
                    })
                    .as_deref()
                    == Some(b"ObjStm")
            })
            .count()
    }

    fn has_signature_shallow(&self) -> bool {
        self.trailers.iter().any(|trailer| {
            trailer.contains_key(b"Perms".as_slice())
                || trailer.contains_key(b"ByteRange".as_slice())
        })
    }

    fn has_signature(&self) -> Result<bool, PdfError> {
        let root = self.get_object(self.root_id()?)?;
        if root
            .as_dictionary()
            .is_some_and(|dictionary| dictionary.contains_key(b"Perms".as_slice()))
        {
            return Ok(true);
        }
        for (number, entry) in &self.entries {
            let id = match entry {
                XrefEntry::Offset { generation, .. } => (*number, *generation),
                XrefEntry::Compressed { .. } => (*number, 0),
            };
            let object = self.get_object(id)?;
            if contains_signature_marker(&object, 0)? {
                return Ok(true);
            }
        }
        Ok(false)
    }
}

fn contains_signature_marker(object: &PdfObject, depth: usize) -> Result<bool, PdfError> {
    if depth > MAX_DEPTH {
        return Err(PdfError::new("PDF signature object is too deeply nested"));
    }
    Ok(match object {
        PdfObject::Dictionary(dictionary) | PdfObject::Stream { dictionary, .. } => {
            dictionary.contains_key(b"ByteRange".as_slice())
                || dictionary
                    .get(b"Type".as_slice())
                    .and_then(PdfObject::as_name)
                    == Some(b"Sig")
                || dictionary
                    .get(b"FT".as_slice())
                    .and_then(PdfObject::as_name)
                    == Some(b"Sig")
                || dictionary
                    .values()
                    .any(|value| contains_signature_marker(value, depth + 1).unwrap_or(true))
        }
        PdfObject::Array(values) => values
            .iter()
            .any(|value| contains_signature_marker(value, depth + 1).unwrap_or(true)),
        _ => false,
    })
}

struct XrefSection {
    entries: BTreeMap<u32, XrefEntry>,
    defined: BTreeSet<u32>,
    trailer: Dictionary,
}

fn parse_xref_section(bytes: &[u8], offset: usize) -> Result<XrefSection, PdfError> {
    let mut parser = Parser::at(bytes, offset)?;
    parser.skip_space_and_comments();
    if parser.consume_keyword(b"xref") {
        parse_classic_xref(parser)
    } else {
        let indirect = parse_indirect_object(bytes, offset, |_| {
            Err(PdfError::new(
                "xref stream entries, including /Length, must be direct objects",
            ))
        })?;
        let PdfObject::Stream { dictionary, data } = indirect.object else {
            return Err(PdfError::new("startxref does not point to xref data"));
        };
        if dictionary
            .get(b"Type".as_slice())
            .and_then(PdfObject::as_name)
            != Some(b"XRef")
        {
            return Err(PdfError::new("startxref stream is not /XRef"));
        }
        let widths = integer_array(
            dictionary
                .get(b"W".as_slice())
                .ok_or_else(|| PdfError::new("xref stream has no /W"))?,
        )?;
        if widths.len() != 3 || widths.iter().sum::<usize>() == 0 {
            return Err(PdfError::new("xref stream /W is invalid"));
        }
        let size = dictionary
            .get(b"Size".as_slice())
            .and_then(PdfObject::as_usize)
            .ok_or_else(|| PdfError::new("xref stream has no valid /Size"))?;
        let indexes = match dictionary.get(b"Index".as_slice()) {
            Some(value) => integer_array(value)?,
            None => vec![0, size],
        };
        if !indexes.len().is_multiple_of(2) {
            return Err(PdfError::new("xref stream /Index is invalid"));
        }
        let decoded = decode_stream(&dictionary, &data)?;
        let entry_width = widths.iter().sum::<usize>();
        let expected = indexes
            .chunks_exact(2)
            .try_fold(0usize, |total, pair| total.checked_add(pair[1]))
            .and_then(|count| count.checked_mul(entry_width))
            .ok_or_else(|| PdfError::new("xref stream size overflow"))?;
        if decoded.len() < expected {
            return Err(PdfError::new("xref stream data is truncated"));
        }
        let mut entries = BTreeMap::new();
        let mut defined = BTreeSet::new();
        let mut cursor = 0;
        for pair in indexes.chunks_exact(2) {
            for number in pair[0]..pair[0] + pair[1] {
                let kind = if widths[0] == 0 {
                    1
                } else {
                    read_be(&decoded[cursor..cursor + widths[0]])
                };
                cursor += widths[0];
                let first = read_be(&decoded[cursor..cursor + widths[1]]);
                cursor += widths[1];
                let second = read_be(&decoded[cursor..cursor + widths[2]]);
                cursor += widths[2];
                let number = u32::try_from(number)
                    .map_err(|_| PdfError::new("xref object number overflow"))?;
                defined.insert(number);
                match kind {
                    1 => {
                        entries.insert(
                            number,
                            XrefEntry::Offset {
                                offset: first,
                                generation: u16::try_from(second)
                                    .map_err(|_| PdfError::new("xref generation overflow"))?,
                            },
                        );
                    }
                    2 => {
                        entries.insert(
                            number,
                            XrefEntry::Compressed {
                                stream: u32::try_from(first)
                                    .map_err(|_| PdfError::new("object stream number overflow"))?,
                                index: second,
                            },
                        );
                    }
                    0 => {}
                    _ => return Err(PdfError::new("xref stream entry type is invalid")),
                }
            }
        }
        Ok(XrefSection {
            entries,
            defined,
            trailer: dictionary,
        })
    }
}

fn parse_classic_xref(mut parser: Parser<'_>) -> Result<XrefSection, PdfError> {
    let mut entries = BTreeMap::new();
    let mut defined = BTreeSet::new();
    loop {
        parser.skip_space_and_comments();
        if parser.consume_keyword(b"trailer") {
            let trailer = parser.parse_object(0)?;
            let PdfObject::Dictionary(trailer) = trailer else {
                return Err(PdfError::new("classic xref trailer is not a dictionary"));
            };
            if trailer.contains_key(b"XRefStm".as_slice()) {
                return Err(PdfError::new(
                    "hybrid-reference PDF cross-reference sections are unsupported",
                ));
            }
            return Ok(XrefSection {
                entries,
                defined,
                trailer,
            });
        }
        let first = parser.read_unsigned()?;
        let count = parser.read_unsigned()?;
        for index in 0..count {
            let offset = parser.read_unsigned()?;
            let generation = parser.read_unsigned()?;
            let status = parser.read_word()?;
            let number = u32::try_from(first + index)
                .map_err(|_| PdfError::new("xref object number overflow"))?;
            defined.insert(number);
            if status == b"n" {
                entries.insert(
                    number,
                    XrefEntry::Offset {
                        offset,
                        generation: u16::try_from(generation)
                            .map_err(|_| PdfError::new("xref generation overflow"))?,
                    },
                );
            } else if status != b"f" {
                return Err(PdfError::new("classic xref entry status is invalid"));
            }
        }
    }
}

struct IndirectObject {
    id: ObjectId,
    object: PdfObject,
}

fn parse_indirect_object<F>(
    bytes: &[u8],
    offset: usize,
    mut resolve_length: F,
) -> Result<IndirectObject, PdfError>
where
    F: FnMut(ObjectId) -> Result<usize, PdfError>,
{
    let mut parser = Parser::at(bytes, offset)?;
    let number = u32::try_from(parser.read_unsigned()?)
        .map_err(|_| PdfError::new("PDF object number overflow"))?;
    let generation = u16::try_from(parser.read_unsigned()?)
        .map_err(|_| PdfError::new("PDF object generation overflow"))?;
    if !parser.consume_keyword(b"obj") {
        return Err(PdfError::new(
            "xref offset does not point to an indirect object",
        ));
    }
    let object = match parser.parse_object(0)? {
        PdfObject::Dictionary(dictionary) => {
            parser.skip_space_and_comments();
            if parser.consume_keyword(b"stream") {
                parser.consume_stream_eol()?;
                let length_object = dictionary
                    .get(b"Length".as_slice())
                    .ok_or_else(|| PdfError::new("PDF stream has no /Length"))?;
                let length = if let Some(length) = length_object.as_usize() {
                    length
                } else if let Some(reference) = length_object.as_reference() {
                    resolve_length(reference)?
                } else {
                    return Err(PdfError::new(
                        "PDF stream /Length is not a non-negative integer or reference",
                    ));
                };
                let end = parser
                    .position
                    .checked_add(length)
                    .filter(|end| *end <= bytes.len())
                    .ok_or_else(|| PdfError::new("PDF stream length exceeds the file"))?;
                let data = bytes[parser.position..end].to_vec();
                parser.position = end;
                parser.skip_space_and_comments();
                if !parser.consume_keyword(b"endstream") {
                    return Err(PdfError::new("PDF stream is missing endstream"));
                }
                PdfObject::Stream { dictionary, data }
            } else {
                PdfObject::Dictionary(dictionary)
            }
        }
        object => object,
    };
    parser.skip_space_and_comments();
    if !parser.consume_keyword(b"endobj") {
        return Err(PdfError::new("PDF indirect object is missing endobj"));
    }
    Ok(IndirectObject {
        id: (number, generation),
        object,
    })
}

struct Parser<'a> {
    bytes: &'a [u8],
    position: usize,
}

impl<'a> Parser<'a> {
    fn new(bytes: &'a [u8]) -> Self {
        Self { bytes, position: 0 }
    }

    fn at(bytes: &'a [u8], position: usize) -> Result<Self, PdfError> {
        if position > bytes.len() {
            return Err(PdfError::new("PDF parser offset exceeds the file"));
        }
        Ok(Self { bytes, position })
    }

    fn parse_object(&mut self, depth: usize) -> Result<PdfObject, PdfError> {
        if depth > MAX_DEPTH {
            return Err(PdfError::new("PDF object nesting is too deep"));
        }
        self.skip_space_and_comments();
        let Some(byte) = self.peek() else {
            return Err(PdfError::new("unexpected end of PDF object"));
        };
        match byte {
            b'/' => self.parse_name().map(PdfObject::Name),
            b'(' => self.parse_literal_string().map(PdfObject::String),
            b'[' => self.parse_array(depth + 1),
            b'<' if self.bytes.get(self.position + 1) == Some(&b'<') => {
                self.parse_dictionary(depth + 1)
            }
            b'<' => self.parse_hex_string().map(PdfObject::String),
            b'n' if self.consume_keyword(b"null") => Ok(PdfObject::Null),
            b't' if self.consume_keyword(b"true") => Ok(PdfObject::Bool(true)),
            b'f' if self.consume_keyword(b"false") => Ok(PdfObject::Bool(false)),
            b'+' | b'-' | b'.' | b'0'..=b'9' => self.parse_number_or_reference(),
            _ => Err(PdfError::new("unsupported or malformed PDF object token")),
        }
    }

    fn parse_number_or_reference(&mut self) -> Result<PdfObject, PdfError> {
        let first = self.read_number_token()?;
        let saved = self.position;
        if !first.contains(['.', '+', '-']) {
            self.skip_space_and_comments();
            if self.peek().is_some_and(|byte| byte.is_ascii_digit()) {
                let second = self.read_number_token()?;
                self.skip_space_and_comments();
                if self.consume_keyword(b"R") {
                    return Ok(PdfObject::Reference((
                        first
                            .parse()
                            .map_err(|_| PdfError::new("PDF reference number is invalid"))?,
                        second
                            .parse()
                            .map_err(|_| PdfError::new("PDF reference generation is invalid"))?,
                    )));
                }
            }
        }
        self.position = saved;
        Ok(PdfObject::Number(first))
    }

    fn parse_array(&mut self, depth: usize) -> Result<PdfObject, PdfError> {
        self.position += 1;
        let mut values = Vec::new();
        loop {
            self.skip_space_and_comments();
            if self.peek() == Some(b']') {
                self.position += 1;
                return Ok(PdfObject::Array(values));
            }
            values.push(self.parse_object(depth)?);
            if values.len() > MAX_OBJECTS {
                return Err(PdfError::new("PDF array is too large"));
            }
        }
    }

    fn parse_dictionary(&mut self, depth: usize) -> Result<PdfObject, PdfError> {
        self.position += 2;
        let mut values = Dictionary::new();
        loop {
            self.skip_space_and_comments();
            if self.bytes.get(self.position..self.position + 2) == Some(b">>") {
                self.position += 2;
                return Ok(PdfObject::Dictionary(values));
            }
            let key = self.parse_name()?;
            let value = self.parse_object(depth)?;
            values.insert(key, value);
            if values.len() > MAX_OBJECTS {
                return Err(PdfError::new("PDF dictionary is too large"));
            }
        }
    }

    fn parse_name(&mut self) -> Result<Vec<u8>, PdfError> {
        if self.peek() != Some(b'/') {
            return Err(PdfError::new("PDF dictionary key is not a name"));
        }
        self.position += 1;
        let mut value = Vec::new();
        while let Some(byte) = self.peek() {
            if is_whitespace(byte) || is_delimiter(byte) {
                break;
            }
            if byte == b'#' {
                let hex = self
                    .bytes
                    .get(self.position + 1..self.position + 3)
                    .ok_or_else(|| PdfError::new("PDF name escape is truncated"))?;
                value.push(hex_byte(hex)?);
                self.position += 3;
            } else {
                value.push(byte);
                self.position += 1;
            }
        }
        Ok(value)
    }

    fn parse_literal_string(&mut self) -> Result<Vec<u8>, PdfError> {
        self.position += 1;
        let mut depth = 1usize;
        let mut value = Vec::new();
        while let Some(byte) = self.next() {
            match byte {
                b'(' => {
                    depth += 1;
                    value.push(byte);
                }
                b')' => {
                    depth -= 1;
                    if depth == 0 {
                        return Ok(value);
                    }
                    value.push(byte);
                }
                b'\\' => {
                    let Some(escaped) = self.next() else {
                        return Err(PdfError::new("PDF literal string escape is truncated"));
                    };
                    match escaped {
                        b'n' => value.push(b'\n'),
                        b'r' => value.push(b'\r'),
                        b't' => value.push(b'\t'),
                        b'b' => value.push(8),
                        b'f' => value.push(12),
                        b'(' | b')' | b'\\' => value.push(escaped),
                        b'\r' => {
                            if self.peek() == Some(b'\n') {
                                self.position += 1;
                            }
                        }
                        b'\n' => {}
                        b'0'..=b'7' => {
                            let mut octal = u16::from(escaped - b'0');
                            for _ in 0..2 {
                                if let Some(next @ b'0'..=b'7') = self.peek() {
                                    self.position += 1;
                                    octal = octal * 8 + u16::from(next - b'0');
                                } else {
                                    break;
                                }
                            }
                            value.push(octal as u8);
                        }
                        other => value.push(other),
                    }
                }
                other => value.push(other),
            }
        }
        Err(PdfError::new("PDF literal string is unterminated"))
    }

    fn parse_hex_string(&mut self) -> Result<Vec<u8>, PdfError> {
        self.position += 1;
        let mut digits = Vec::new();
        loop {
            let byte = self
                .next()
                .ok_or_else(|| PdfError::new("PDF hex string is unterminated"))?;
            if byte == b'>' {
                break;
            }
            if !is_whitespace(byte) {
                if !byte.is_ascii_hexdigit() {
                    return Err(PdfError::new("PDF hex string contains invalid data"));
                }
                digits.push(byte);
            }
        }
        if !digits.len().is_multiple_of(2) {
            digits.push(b'0');
        }
        digits.chunks_exact(2).map(hex_byte).collect()
    }

    fn read_unsigned(&mut self) -> Result<usize, PdfError> {
        self.skip_space_and_comments();
        let start = self.position;
        while self.peek().is_some_and(|byte| byte.is_ascii_digit()) {
            self.position += 1;
        }
        if start == self.position {
            return Err(PdfError::new("expected unsigned PDF integer"));
        }
        std::str::from_utf8(&self.bytes[start..self.position])
            .ok()
            .and_then(|value| value.parse().ok())
            .ok_or_else(|| PdfError::new("PDF unsigned integer is invalid"))
    }

    fn read_number_token(&mut self) -> Result<String, PdfError> {
        self.skip_space_and_comments();
        let start = self.position;
        if matches!(self.peek(), Some(b'+' | b'-')) {
            self.position += 1;
        }
        let mut digits = 0;
        while self.peek().is_some_and(|byte| byte.is_ascii_digit()) {
            digits += 1;
            self.position += 1;
        }
        if self.peek() == Some(b'.') {
            self.position += 1;
            while self.peek().is_some_and(|byte| byte.is_ascii_digit()) {
                digits += 1;
                self.position += 1;
            }
        }
        if digits == 0 {
            return Err(PdfError::new("PDF number is invalid"));
        }
        String::from_utf8(self.bytes[start..self.position].to_vec())
            .map_err(|_| PdfError::new("PDF number is not ASCII"))
    }

    fn read_word(&mut self) -> Result<Vec<u8>, PdfError> {
        self.skip_space_and_comments();
        let start = self.position;
        while self
            .peek()
            .is_some_and(|byte| !is_whitespace(byte) && !is_delimiter(byte))
        {
            self.position += 1;
        }
        if start == self.position {
            return Err(PdfError::new("expected PDF keyword"));
        }
        Ok(self.bytes[start..self.position].to_vec())
    }

    fn consume_keyword(&mut self, keyword: &[u8]) -> bool {
        self.skip_space_and_comments();
        if self.bytes.get(self.position..self.position + keyword.len()) != Some(keyword) {
            return false;
        }
        let end = self.position + keyword.len();
        if self
            .bytes
            .get(end)
            .is_some_and(|byte| !is_whitespace(*byte) && !is_delimiter(*byte))
        {
            return false;
        }
        self.position = end;
        true
    }

    fn consume_stream_eol(&mut self) -> Result<(), PdfError> {
        match (self.next(), self.peek()) {
            (Some(b'\r'), Some(b'\n')) => {
                self.position += 1;
                Ok(())
            }
            (Some(b'\r' | b'\n'), _) => Ok(()),
            _ => Err(PdfError::new("PDF stream keyword has no line ending")),
        }
    }

    fn skip_space_and_comments(&mut self) {
        loop {
            while self.peek().is_some_and(is_whitespace) {
                self.position += 1;
            }
            if self.peek() != Some(b'%') {
                break;
            }
            while self
                .peek()
                .is_some_and(|byte| !matches!(byte, b'\r' | b'\n'))
            {
                self.position += 1;
            }
        }
    }

    fn peek(&self) -> Option<u8> {
        self.bytes.get(self.position).copied()
    }

    fn next(&mut self) -> Option<u8> {
        let value = self.peek()?;
        self.position += 1;
        Some(value)
    }
}

fn serialize_object(
    object: &PdfObject,
    output: &mut Vec<u8>,
    depth: usize,
) -> Result<(), PdfError> {
    if depth > MAX_DEPTH {
        return Err(PdfError::new(
            "PDF object is too deeply nested to serialize",
        ));
    }
    match object {
        PdfObject::Null => output.extend_from_slice(b"null"),
        PdfObject::Bool(value) => output.extend_from_slice(if *value { b"true" } else { b"false" }),
        PdfObject::Number(value) => output.extend_from_slice(value.as_bytes()),
        PdfObject::Name(value) => serialize_name(value, output),
        PdfObject::String(value) => {
            output.push(b'<');
            for byte in value {
                output.extend_from_slice(format!("{byte:02X}").as_bytes());
            }
            output.push(b'>');
        }
        PdfObject::Array(values) => {
            output.push(b'[');
            for (index, value) in values.iter().enumerate() {
                if index > 0 {
                    output.push(b' ');
                }
                serialize_object(value, output, depth + 1)?;
            }
            output.push(b']');
        }
        PdfObject::Dictionary(dictionary) => serialize_dictionary(dictionary, output, depth + 1)?,
        PdfObject::Reference((number, generation)) => {
            output.extend_from_slice(format!("{number} {generation} R").as_bytes());
        }
        PdfObject::Stream { dictionary, data } => {
            let mut dictionary = dictionary.clone();
            dictionary.insert(
                b"Length".to_vec(),
                PdfObject::Number(data.len().to_string()),
            );
            serialize_dictionary(&dictionary, output, depth + 1)?;
            output.extend_from_slice(b"\nstream\n");
            output.extend_from_slice(data);
            output.extend_from_slice(b"\nendstream");
        }
    }
    Ok(())
}

fn serialize_dictionary(
    dictionary: &Dictionary,
    output: &mut Vec<u8>,
    depth: usize,
) -> Result<(), PdfError> {
    output.extend_from_slice(b"<<");
    for (key, value) in dictionary {
        output.push(b' ');
        serialize_name(key, output);
        output.push(b' ');
        serialize_object(value, output, depth + 1)?;
    }
    output.extend_from_slice(b" >>");
    Ok(())
}

fn serialize_name(name: &[u8], output: &mut Vec<u8>) {
    output.push(b'/');
    for byte in name {
        if *byte < 33 || *byte > 126 || is_delimiter(*byte) || *byte == b'#' {
            output.extend_from_slice(format!("#{byte:02X}").as_bytes());
        } else {
            output.push(*byte);
        }
    }
}

fn decode_stream(dictionary: &Dictionary, data: &[u8]) -> Result<Vec<u8>, PdfError> {
    let filters = match dictionary.get(b"Filter".as_slice()) {
        None => Vec::new(),
        Some(PdfObject::Name(name)) => vec![name.clone()],
        Some(PdfObject::Array(values)) => values
            .iter()
            .map(|value| {
                value
                    .as_name()
                    .map(<[u8]>::to_vec)
                    .ok_or_else(|| PdfError::new("PDF stream filter array is invalid"))
            })
            .collect::<Result<Vec<_>, _>>()?,
        Some(_) => return Err(PdfError::new("PDF stream filter is invalid")),
    };
    let mut decoded = data.to_vec();
    for filter in filters {
        match filter.as_slice() {
            b"FlateDecode" | b"Fl" => {
                let decoder = ZlibDecoder::new(decoded.as_slice());
                let mut next = Vec::new();
                decoder
                    .take(MAX_DECOMPRESSED_STREAM as u64 + 1)
                    .read_to_end(&mut next)
                    .map_err(|error| {
                        PdfError::new(format!("PDF Flate stream is invalid: {error}"))
                    })?;
                if next.len() > MAX_DECOMPRESSED_STREAM {
                    return Err(PdfError::new("decompressed PDF stream exceeds the limit"));
                }
                decoded = next;
            }
            _ => {
                return Err(PdfError::new(format!(
                    "unsupported PDF stream filter /{}",
                    String::from_utf8_lossy(&filter)
                )));
            }
        }
    }
    apply_predictor(dictionary, decoded)
}

fn apply_predictor(dictionary: &Dictionary, data: Vec<u8>) -> Result<Vec<u8>, PdfError> {
    let Some(parameters) = dictionary.get(b"DecodeParms".as_slice()) else {
        return Ok(data);
    };
    let parameters = match parameters {
        PdfObject::Dictionary(value) => value,
        PdfObject::Array(values) if values.len() == 1 => values[0]
            .as_dictionary()
            .ok_or_else(|| PdfError::new("PDF DecodeParms is invalid"))?,
        PdfObject::Null => return Ok(data),
        _ => return Err(PdfError::new("PDF DecodeParms is invalid")),
    };
    let predictor = parameters
        .get(b"Predictor".as_slice())
        .and_then(PdfObject::as_usize)
        .unwrap_or(1);
    if predictor <= 1 {
        return Ok(data);
    }
    if !(10..=15).contains(&predictor) {
        return Err(PdfError::new("unsupported PDF stream predictor"));
    }
    let columns = parameters
        .get(b"Columns".as_slice())
        .and_then(PdfObject::as_usize)
        .unwrap_or(1);
    let colors = parameters
        .get(b"Colors".as_slice())
        .and_then(PdfObject::as_usize)
        .unwrap_or(1);
    let bits = parameters
        .get(b"BitsPerComponent".as_slice())
        .and_then(PdfObject::as_usize)
        .unwrap_or(8);
    if bits != 8 {
        return Err(PdfError::new("unsupported PDF predictor bit depth"));
    }
    let bytes_per_pixel = colors.max(1);
    let row_bytes = columns
        .checked_mul(colors)
        .ok_or_else(|| PdfError::new("PDF predictor row size overflow"))?;
    let stride = row_bytes + 1;
    if stride == 0 || !data.len().is_multiple_of(stride) {
        return Err(PdfError::new("PDF predictor data is truncated"));
    }
    let mut output = Vec::with_capacity(data.len() / stride * row_bytes);
    let mut previous = vec![0u8; row_bytes];
    for encoded in data.chunks_exact(stride) {
        let filter = encoded[0];
        let mut row = encoded[1..].to_vec();
        for index in 0..row_bytes {
            let left = if index >= bytes_per_pixel {
                row[index - bytes_per_pixel]
            } else {
                0
            };
            let up = previous[index];
            let upper_left = if index >= bytes_per_pixel {
                previous[index - bytes_per_pixel]
            } else {
                0
            };
            row[index] = match filter {
                0 => row[index],
                1 => row[index].wrapping_add(left),
                2 => row[index].wrapping_add(up),
                3 => row[index].wrapping_add(((u16::from(left) + u16::from(up)) / 2) as u8),
                4 => row[index].wrapping_add(paeth(left, up, upper_left)),
                _ => return Err(PdfError::new("PDF predictor filter is invalid")),
            };
        }
        output.extend_from_slice(&row);
        previous = row;
    }
    Ok(output)
}

fn paeth(left: u8, up: u8, upper_left: u8) -> u8 {
    let estimate = i32::from(left) + i32::from(up) - i32::from(upper_left);
    let left_distance = (estimate - i32::from(left)).abs();
    let up_distance = (estimate - i32::from(up)).abs();
    let upper_left_distance = (estimate - i32::from(upper_left)).abs();
    if left_distance <= up_distance && left_distance <= upper_left_distance {
        left
    } else if up_distance <= upper_left_distance {
        up
    } else {
        upper_left
    }
}

fn find_startxref(bytes: &[u8]) -> Result<usize, PdfError> {
    let marker = b"startxref";
    let position = bytes
        .windows(marker.len())
        .rposition(|window| window == marker)
        .ok_or_else(|| PdfError::new("PDF startxref is missing"))?;
    let mut parser = Parser::at(bytes, position + marker.len())?;
    parser.read_unsigned()
}

fn integer_array(object: &PdfObject) -> Result<Vec<usize>, PdfError> {
    object
        .as_array()
        .ok_or_else(|| PdfError::new("PDF integer array is invalid"))?
        .iter()
        .map(|value| {
            value
                .as_usize()
                .ok_or_else(|| PdfError::new("PDF integer array contains a non-integer"))
        })
        .collect()
}

fn box_values(object: &PdfObject) -> Option<Vec<String>> {
    let values = object.as_array()?;
    (values.len() == 4).then(|| {
        values
            .iter()
            .map(|value| match value {
                PdfObject::Number(number) => Some(number.clone()),
                _ => None,
            })
            .collect::<Option<Vec<_>>>()
    })?
}

fn object_text(object: &PdfObject) -> String {
    match object {
        PdfObject::String(value) | PdfObject::Name(value) => {
            String::from_utf8_lossy(value).into_owned()
        }
        _ => String::new(),
    }
}

fn read_bounded(path: &Path) -> Result<Vec<u8>, PdfError> {
    let metadata = fs::symlink_metadata(path)
        .map_err(|error| PdfError::new(format!("PDF metadata failed: {error}")))?;
    if !metadata.file_type().is_file() || metadata.len() == 0 || metadata.len() > MAX_PDF_BYTES {
        return Err(PdfError::new("PDF file type or size is unsupported"));
    }
    fs::read(path).map_err(|error| PdfError::new(format!("PDF read failed: {error}")))
}

fn read_be(bytes: &[u8]) -> usize {
    bytes
        .iter()
        .fold(0usize, |value, byte| (value << 8) | usize::from(*byte))
}

fn hex_byte(bytes: &[u8]) -> Result<u8, PdfError> {
    if bytes.len() != 2 {
        return Err(PdfError::new("PDF hex byte is truncated"));
    }
    let value = |byte: u8| match byte {
        b'0'..=b'9' => Some(byte - b'0'),
        b'a'..=b'f' => Some(byte - b'a' + 10),
        b'A'..=b'F' => Some(byte - b'A' + 10),
        _ => None,
    };
    value(bytes[0])
        .and_then(|high| value(bytes[1]).map(|low| high * 16 + low))
        .ok_or_else(|| PdfError::new("PDF hex byte is invalid"))
}

fn sha256(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}

fn is_whitespace(byte: u8) -> bool {
    matches!(byte, 0 | b'\t' | b'\n' | 12 | b'\r' | b' ')
}

fn is_delimiter(byte: u8) -> bool {
    matches!(
        byte,
        b'(' | b')' | b'<' | b'>' | b'[' | b']' | b'{' | b'}' | b'/' | b'%'
    )
}

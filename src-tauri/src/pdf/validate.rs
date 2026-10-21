use super::*;

pub(super) fn validate_extracted_pdf(
    path: &Path,
    source: &ParsedPdf,
    selected: &[ObjectId],
) -> Result<(), PdfError> {
    let before = source.inspect()?;
    let expected = selected
        .iter()
        .map(|id| source.inspect_page(*id))
        .collect::<Result<Vec<_>, _>>()?;
    let after = ParsedPdf::load(path)?.inspect()?;
    let metadata_matches =
        after.catalog_entry_hashes.get("Metadata") == before.catalog_entry_hashes.get("Metadata");
    let pages_match = after.pages.iter().zip(expected).all(|(actual, expected)| {
        actual.content_hash == expected.content_hash
            && actual.media_box == expected.media_box
            && actual.crop_box == expected.crop_box
            && actual.rotation == expected.rotation
            && actual.annotations == expected.annotations
            && actual.links == expected.links
            && actual.widgets == expected.widgets
    });
    if after.page_count != selected.len() || !metadata_matches || !pages_match {
        return Err(PdfError::new(
            "extracted PDF failed page-fidelity validation",
        ));
    }
    Ok(())
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

pub(super) fn build_incremental_rotations(
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

pub(super) fn build_incremental_appendix(
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

pub(super) fn validate_rotated(
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

pub(super) fn reject_unsupported(facts: &PdfInspection, label: &str) -> Result<(), PdfError> {
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

pub(super) fn validate_completed(
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

pub(super) fn build_incremental_replacement(
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
        target_pages: HashMap::from([(signed_page, archive_page)]),
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

pub(super) struct Importer<'a> {
    pub(super) source: &'a ParsedPdf,
    pub(super) target_pages: HashMap<ObjectId, ObjectId>,
    pub(super) source_page_ids: HashSet<ObjectId>,
    pub(super) next_id: u32,
    pub(super) assigned: HashMap<ObjectId, ObjectId>,
    pub(super) objects: BTreeMap<ObjectId, PdfObject>,
    pub(super) visiting: HashSet<ObjectId>,
}

impl Importer<'_> {
    pub(super) fn import_object(
        &mut self,
        object: PdfObject,
        depth: usize,
    ) -> Result<PdfObject, PdfError> {
        if depth > MAX_DEPTH {
            return Err(PdfError::new(
                "signed page object graph is too deeply nested",
            ));
        }
        Ok(match object {
            PdfObject::Reference(id) if self.target_pages.contains_key(&id) => {
                PdfObject::Reference(self.target_pages[&id])
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
                    .filter(|(key, _)| key.as_slice() != b"Length")
                    .map(|(key, value)| Ok((key, self.import_object(value, depth + 1)?)))
                    .collect::<Result<_, PdfError>>()?,
                data,
            },
            value => value,
        })
    }
}

pub(super) fn materialize_inherited_page_values(
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

use super::*;

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

/// Writes a standalone PDF containing only the approved source pages.
/// Unselected page objects and catalog-level content are never copied.
pub fn extract_pages(
    source_path: &Path,
    page_indexes: &[usize],
    output_path: &Path,
) -> Result<(), PdfError> {
    if output_path.exists() {
        return Err(PdfError::new("PDF extraction destination already exists"));
    }
    if page_indexes.is_empty() {
        return Err(PdfError::new("no PDF pages were approved for extraction"));
    }
    let source = ParsedPdf::load(source_path)?;
    let before = source.inspect()?;
    if !extraction_supported(&before) {
        return Err(PdfError::new(
            "PDF contains features that cannot be safely separated",
        ));
    }
    let pages = source.pages()?;
    let mut selected = Vec::with_capacity(page_indexes.len());
    let mut unique = HashSet::new();
    for &index in page_indexes {
        if index >= pages.len() || !unique.insert(index) {
            return Err(PdfError::new("approved PDF page indexes are invalid"));
        }
        selected.push(pages[index]);
    }
    let bytes = build_extracted_pdf(&source, &selected)?;
    let parent = output_path
        .parent()
        .ok_or_else(|| PdfError::new("PDF extraction destination has no parent"))?;
    if !parent.is_dir() {
        return Err(PdfError::new(
            "PDF extraction destination parent is missing",
        ));
    }
    let mut output = fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(output_path)
        .map_err(|error| PdfError::new(format!("PDF extraction output create failed: {error}")))?;
    let result = (|| {
        output
            .write_all(&bytes)
            .and_then(|()| output.sync_all())
            .map_err(|error| {
                PdfError::new(format!("PDF extraction output write failed: {error}"))
            })?;
        drop(output);
        validate_extracted_pdf(output_path, &source, &selected)
    })();
    if result.is_err() {
        let _ = fs::remove_file(output_path);
    }
    result
}

pub(crate) fn extraction_supported(facts: &PdfInspection) -> bool {
    !facts.encrypted
        && !facts.signed
        && facts.embedded_file_hashes.is_empty()
        // Metadata is copied through the bounded importer; stream lengths are
        // recomputed from the preserved stream bytes during serialization.
        && (!facts.catalog_entry_hashes.contains_key("Outlines") || facts.empty_outline_tree)
        && ![
            "StructTreeRoot",
            "OCProperties",
            "AcroForm",
            "Names",
            "PageLabels",
            "OpenAction",
            "ViewerPreferences",
            "Dests",
            "OutputIntents",
        ]
        .iter()
        .any(|key| facts.catalog_entry_hashes.contains_key(*key))
        && facts
            .pages
            .iter()
            .all(|page| page.annotations == 0 && page.widgets == 0)
}

pub(super) fn build_extracted_pdf(
    source: &ParsedPdf,
    selected: &[ObjectId],
) -> Result<Vec<u8>, PdfError> {
    let all_pages = source.pages()?;
    let selected_targets = selected
        .iter()
        .enumerate()
        .map(|(index, source_id)| (*source_id, ((index + 3) as u32, 0)))
        .collect::<HashMap<_, _>>();
    let selected_sources = all_pages.iter().copied().collect::<HashSet<_>>();
    let mut importer = Importer {
        source,
        target_pages: selected_targets.clone(),
        source_page_ids: selected_sources,
        next_id: (selected.len() + 3) as u32,
        assigned: HashMap::new(),
        objects: BTreeMap::new(),
        visiting: HashSet::new(),
    };
    let mut writes = BTreeMap::new();
    for source_page in selected {
        let mut dictionary = source
            .get_object(*source_page)?
            .as_dictionary()
            .cloned()
            .ok_or_else(|| PdfError::new("selected PDF page is not a dictionary"))?;
        materialize_inherited_page_values(source, *source_page, &mut dictionary)?;
        dictionary.remove(b"Parent".as_slice());
        let imported = importer.import_object(PdfObject::Dictionary(dictionary), 0)?;
        let mut dictionary = match imported {
            PdfObject::Dictionary(dictionary) => dictionary,
            _ => {
                return Err(PdfError::new(
                    "selected PDF page import changed object type",
                ));
            }
        };
        dictionary.insert(b"Parent".to_vec(), PdfObject::Reference((2, 0)));
        writes.insert(
            selected_targets[source_page],
            PdfObject::Dictionary(dictionary),
        );
    }
    let info = source
        .trailers
        .first()
        .and_then(|trailer| trailer.get(b"Info".as_slice()))
        .cloned()
        .map(|info| importer.import_object(info, 0))
        .transpose()?;
    let metadata = source
        .get_object(source.root_id()?)?
        .as_dictionary()
        .and_then(|catalog| catalog.get(b"Metadata".as_slice()))
        .cloned()
        .map(|metadata| importer.import_object(metadata, 0))
        .transpose()?;
    writes.extend(importer.objects);
    let kids = selected
        .iter()
        .map(|page| PdfObject::Reference(selected_targets[page]))
        .collect();
    let mut pages = Dictionary::new();
    pages.insert(b"Type".to_vec(), PdfObject::Name(b"Pages".to_vec()));
    pages.insert(b"Kids".to_vec(), PdfObject::Array(kids));
    pages.insert(
        b"Count".to_vec(),
        PdfObject::Number(selected.len().to_string()),
    );
    writes.insert((2, 0), PdfObject::Dictionary(pages));
    let mut catalog = Dictionary::new();
    catalog.insert(b"Type".to_vec(), PdfObject::Name(b"Catalog".to_vec()));
    catalog.insert(b"Pages".to_vec(), PdfObject::Reference((2, 0)));
    if let Some(metadata) = metadata {
        catalog.insert(b"Metadata".to_vec(), metadata);
    }
    writes.insert((1, 0), PdfObject::Dictionary(catalog));
    if writes.len() > MAX_OBJECTS {
        return Err(PdfError::new("extracted PDF object graph is too large"));
    }

    let mut bytes = b"%PDF-1.7\n%\xE2\xE3\xCF\xD3\n".to_vec();
    let mut offsets = BTreeMap::new();
    for (id, object) in &writes {
        offsets.insert(*id, bytes.len());
        bytes.extend_from_slice(format!("{} {} obj\n", id.0, id.1).as_bytes());
        serialize_object(object, &mut bytes, 0)?;
        bytes.extend_from_slice(b"\nendobj\n");
    }
    let xref = bytes.len();
    let size = offsets.keys().map(|id| id.0).max().unwrap_or(0) + 1;
    bytes.extend_from_slice(format!("xref\n0 {size}\n0000000000 65535 f \n").as_bytes());
    for number in 1..size {
        let offset = offsets
            .get(&(number, 0))
            .ok_or_else(|| PdfError::new("extracted PDF object roster is incomplete"))?;
        bytes.extend_from_slice(format!("{offset:010} 00000 n \n").as_bytes());
    }
    let mut trailer = Dictionary::new();
    trailer.insert(b"Size".to_vec(), PdfObject::Number(size.to_string()));
    trailer.insert(b"Root".to_vec(), PdfObject::Reference((1, 0)));
    if let Some(info) = info {
        trailer.insert(b"Info".to_vec(), info);
    }
    bytes.extend_from_slice(b"trailer\n");
    serialize_object(&PdfObject::Dictionary(trailer), &mut bytes, 0)?;
    bytes.extend_from_slice(format!("\nstartxref\n{xref}\n%%EOF\n").as_bytes());
    Ok(bytes)
}

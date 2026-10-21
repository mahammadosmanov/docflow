use super::*;

impl ParsedPdf {
    pub(super) fn load(path: &Path) -> Result<Self, PdfError> {
        Self::parse(read_bounded(path)?)
    }

    pub(super) fn parse(bytes: impl Into<Arc<[u8]>>) -> Result<Self, PdfError> {
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

    pub(super) fn get_object(&self, id: ObjectId) -> Result<PdfObject, PdfError> {
        self.get_object_resolving_lengths(id, &mut HashSet::new())
    }

    pub(super) fn get_object_resolving_lengths(
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

    pub(super) fn get_compressed_object(
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

    pub(super) fn root_id(&self) -> Result<ObjectId, PdfError> {
        self.trailers
            .iter()
            .find_map(|trailer| trailer.get(b"Root".as_slice()))
            .and_then(PdfObject::as_reference)
            .ok_or_else(|| PdfError::new("PDF catalog reference is missing"))
    }

    pub(super) fn pages(&self) -> Result<Vec<ObjectId>, PdfError> {
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

    pub(super) fn walk_pages(
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

    pub(super) fn inherited_value(
        &self,
        page: ObjectId,
        key: &[u8],
    ) -> Result<Option<PdfObject>, PdfError> {
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

    pub(super) fn inspect(&self) -> Result<PdfInspection, PdfError> {
        let encrypted = self
            .trailers
            .iter()
            .any(|trailer| trailer.contains_key(b"Encrypt".as_slice()));
        if encrypted {
            return Ok(PdfInspection {
                page_count: 0,
                pages: Vec::new(),
                catalog_entry_hashes: BTreeMap::new(),
                metadata_xml_self_contained: false,
                empty_outline_tree: false,
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
        let metadata_xml_self_contained = self.metadata_xml_self_contained(catalog)?;
        let empty_outline_tree = self.empty_outline_tree(catalog)?;
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
                let hash = if key == b"Metadata" {
                    if let PdfObject::Reference(id) = value {
                        self.metadata_graph_hash(&self.get_object(*id)?)?
                    } else {
                        self.metadata_graph_hash(value)?
                    }
                } else {
                    self.graph_hash(value)?
                };
                catalog_entry_hashes.insert(String::from_utf8_lossy(key).into_owned(), hash);
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
            metadata_xml_self_contained,
            empty_outline_tree,
            info_hash,
            embedded_file_hashes: self.embedded_files(catalog)?,
            incremental_revisions: self.revisions,
            object_stream_count: self.object_stream_count(),
            encrypted,
            signed,
        })
    }

    pub(super) fn metadata_xml_self_contained(
        &self,
        catalog: &Dictionary,
    ) -> Result<bool, PdfError> {
        let Some(PdfObject::Reference(id)) = catalog.get(b"Metadata".as_slice()) else {
            return Ok(false);
        };
        let object = self.get_object(*id)?;
        let PdfObject::Stream { dictionary, .. } = object else {
            return Ok(false);
        };
        let is_xml_metadata = dictionary
            .get(b"Type".as_slice())
            .and_then(PdfObject::as_name)
            == Some(b"Metadata")
            && dictionary
                .get(b"Subtype".as_slice())
                .and_then(PdfObject::as_name)
                == Some(b"XML");
        Ok(is_xml_metadata && !dictionary.values().any(contains_reference))
    }

    pub(super) fn empty_outline_tree(&self, catalog: &Dictionary) -> Result<bool, PdfError> {
        let Some(value) = catalog.get(b"Outlines".as_slice()) else {
            return Ok(true);
        };
        let Some(id) = value.as_reference() else {
            return Ok(false);
        };
        let object = self.get_object(id)?;
        let Some(dictionary) = object.as_dictionary() else {
            return Ok(false);
        };
        Ok(dictionary
            .get(b"Type".as_slice())
            .and_then(PdfObject::as_name)
            == Some(b"Outlines")
            && dictionary
                .get(b"Count".as_slice())
                .and_then(PdfObject::as_i64)
                == Some(0)
            && !dictionary.contains_key(b"First".as_slice())
            && !dictionary.contains_key(b"Last".as_slice()))
    }

    pub(super) fn inspect_page(&self, id: ObjectId) -> Result<PdfPageInspection, PdfError> {
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

    pub(super) fn resolve(&self, object: &PdfObject) -> Result<PdfObject, PdfError> {
        match object {
            PdfObject::Reference(id) => self.get_object(*id),
            value => Ok(value.clone()),
        }
    }

    pub(super) fn resolve_array(&self, object: &PdfObject) -> Result<Vec<PdfObject>, PdfError> {
        self.resolve(object)?
            .as_array()
            .map(<[PdfObject]>::to_vec)
            .ok_or_else(|| PdfError::new("PDF object is not an array"))
    }

    pub(super) fn content_hash(&self, object: &PdfObject) -> Result<String, PdfError> {
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

    pub(super) fn resolve_array_or_single(
        &self,
        object: &PdfObject,
    ) -> Result<Vec<PdfObject>, PdfError> {
        let resolved = self.resolve(object)?;
        Ok(match resolved {
            PdfObject::Array(values) => values,
            value => vec![value],
        })
    }

    pub(super) fn object_hash(&self, id: ObjectId) -> Result<String, PdfError> {
        let mut serialized = Vec::new();
        serialize_object(&self.get_object(id)?, &mut serialized, 0)?;
        Ok(sha256(&serialized))
    }

    pub(super) fn graph_hash(&self, object: &PdfObject) -> Result<String, PdfError> {
        let mut digest = Sha256::new();
        let mut seen = BTreeSet::new();
        self.hash_graph(object, &mut digest, &mut seen, 0)?;
        Ok(format!("{:x}", digest.finalize()))
    }

    pub(super) fn metadata_graph_hash(&self, object: &PdfObject) -> Result<String, PdfError> {
        let mut digest = Sha256::new();
        self.hash_metadata_graph(object, &mut digest, &mut BTreeSet::new(), 0)?;
        Ok(format!("{:x}", digest.finalize()))
    }

    pub(super) fn hash_metadata_graph(
        &self,
        object: &PdfObject,
        digest: &mut Sha256,
        seen: &mut BTreeSet<ObjectId>,
        depth: usize,
    ) -> Result<(), PdfError> {
        if depth > MAX_DEPTH {
            return Err(PdfError::new("PDF metadata graph is too deeply nested"));
        }
        match object {
            PdfObject::Reference(id) => {
                digest.update(b"reference");
                if seen.insert(*id) {
                    self.hash_metadata_graph(&self.get_object(*id)?, digest, seen, depth + 1)?;
                } else {
                    digest.update(b"already-visited");
                }
            }
            PdfObject::Array(values) => {
                digest.update(b"array");
                for value in values {
                    self.hash_metadata_graph(value, digest, seen, depth + 1)?;
                }
            }
            PdfObject::Dictionary(values) => {
                digest.update(b"dictionary");
                for (key, value) in values {
                    digest.update((key.len() as u64).to_be_bytes());
                    digest.update(key);
                    self.hash_metadata_graph(value, digest, seen, depth + 1)?;
                }
            }
            PdfObject::Stream { dictionary, data } => {
                digest.update(b"stream");
                digest.update((data.len() as u64).to_be_bytes());
                digest.update(data);
                for (key, value) in dictionary {
                    if key.as_slice() == b"Length" {
                        continue;
                    }
                    digest.update((key.len() as u64).to_be_bytes());
                    digest.update(key);
                    self.hash_metadata_graph(value, digest, seen, depth + 1)?;
                }
            }
            scalar => {
                let mut bytes = Vec::new();
                serialize_object(scalar, &mut bytes, depth)?;
                digest.update((bytes.len() as u64).to_be_bytes());
                digest.update(bytes);
            }
        }
        Ok(())
    }

    pub(super) fn hash_graph(
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

    pub(super) fn embedded_files(
        &self,
        catalog: &Dictionary,
    ) -> Result<BTreeMap<String, String>, PdfError> {
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

    pub(super) fn collect_name_tree(
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
            for pair in values.as_chunks::<2>().0 {
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

    pub(super) fn object_stream_count(&self) -> usize {
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

    pub(super) fn has_signature_shallow(&self) -> bool {
        self.trailers.iter().any(|trailer| {
            trailer.contains_key(b"Perms".as_slice())
                || trailer.contains_key(b"ByteRange".as_slice())
        })
    }

    pub(super) fn has_signature(&self) -> Result<bool, PdfError> {
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

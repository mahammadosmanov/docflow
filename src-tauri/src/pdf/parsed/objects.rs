use super::*;

impl ParsedPdf {
    pub(in crate::pdf) fn load(path: &Path) -> Result<Self, PdfError> {
        Self::parse(read_bounded(path)?)
    }

    pub(in crate::pdf) fn parse(bytes: impl Into<Arc<[u8]>>) -> Result<Self, PdfError> {
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

    pub(in crate::pdf) fn get_object(&self, id: ObjectId) -> Result<PdfObject, PdfError> {
        self.get_object_resolving_lengths(id, &mut HashSet::new())
    }

    pub(in crate::pdf) fn get_object_resolving_lengths(
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

    pub(in crate::pdf) fn get_compressed_object(
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

    pub(in crate::pdf) fn root_id(&self) -> Result<ObjectId, PdfError> {
        self.trailers
            .iter()
            .find_map(|trailer| trailer.get(b"Root".as_slice()))
            .and_then(PdfObject::as_reference)
            .ok_or_else(|| PdfError::new("PDF catalog reference is missing"))
    }

    pub(in crate::pdf) fn pages(&self) -> Result<Vec<ObjectId>, PdfError> {
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

    pub(in crate::pdf) fn walk_pages(
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

    pub(in crate::pdf) fn inherited_value(
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
}

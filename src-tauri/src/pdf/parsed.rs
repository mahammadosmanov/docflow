use super::*;

mod hashing;
mod inspection;
mod objects;

impl ParsedPdf {
    pub(in crate::pdf) fn embedded_files(
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

    pub(in crate::pdf) fn collect_name_tree(
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

    pub(in crate::pdf) fn object_stream_count(&self) -> usize {
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

    pub(in crate::pdf) fn has_signature_shallow(&self) -> bool {
        self.trailers.iter().any(|trailer| {
            trailer.contains_key(b"Perms".as_slice())
                || trailer.contains_key(b"ByteRange".as_slice())
        })
    }

    pub(in crate::pdf) fn has_signature(&self) -> Result<bool, PdfError> {
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

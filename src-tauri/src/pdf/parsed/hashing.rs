use super::*;

impl ParsedPdf {
    pub(in crate::pdf) fn resolve(&self, object: &PdfObject) -> Result<PdfObject, PdfError> {
        match object {
            PdfObject::Reference(id) => self.get_object(*id),
            value => Ok(value.clone()),
        }
    }

    pub(in crate::pdf) fn resolve_array(
        &self,
        object: &PdfObject,
    ) -> Result<Vec<PdfObject>, PdfError> {
        self.resolve(object)?
            .as_array()
            .map(<[PdfObject]>::to_vec)
            .ok_or_else(|| PdfError::new("PDF object is not an array"))
    }

    pub(in crate::pdf) fn content_hash(&self, object: &PdfObject) -> Result<String, PdfError> {
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

    pub(in crate::pdf) fn resolve_array_or_single(
        &self,
        object: &PdfObject,
    ) -> Result<Vec<PdfObject>, PdfError> {
        let resolved = self.resolve(object)?;
        Ok(match resolved {
            PdfObject::Array(values) => values,
            value => vec![value],
        })
    }

    pub(in crate::pdf) fn object_hash(&self, id: ObjectId) -> Result<String, PdfError> {
        let mut serialized = Vec::new();
        serialize_object(&self.get_object(id)?, &mut serialized, 0)?;
        Ok(sha256(&serialized))
    }

    pub(in crate::pdf) fn graph_hash(&self, object: &PdfObject) -> Result<String, PdfError> {
        let mut digest = Sha256::new();
        let mut seen = BTreeSet::new();
        self.hash_graph(object, &mut digest, &mut seen, 0)?;
        Ok(format!("{:x}", digest.finalize()))
    }

    pub(in crate::pdf) fn metadata_graph_hash(
        &self,
        object: &PdfObject,
    ) -> Result<String, PdfError> {
        let mut digest = Sha256::new();
        self.hash_metadata_graph(object, &mut digest, &mut BTreeSet::new(), 0)?;
        Ok(format!("{:x}", digest.finalize()))
    }

    pub(in crate::pdf) fn hash_metadata_graph(
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

    pub(in crate::pdf) fn hash_graph(
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
}

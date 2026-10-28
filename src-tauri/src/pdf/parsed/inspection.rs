use super::*;

impl ParsedPdf {
    pub(in crate::pdf) fn inspect(&self) -> Result<PdfInspection, PdfError> {
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

    pub(in crate::pdf) fn metadata_xml_self_contained(
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

    pub(in crate::pdf) fn empty_outline_tree(
        &self,
        catalog: &Dictionary,
    ) -> Result<bool, PdfError> {
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

    pub(in crate::pdf) fn inspect_page(&self, id: ObjectId) -> Result<PdfPageInspection, PdfError> {
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
}

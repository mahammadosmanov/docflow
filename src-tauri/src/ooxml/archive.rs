use super::*;

pub(super) fn validate_source(source: &Path) -> Result<(), OoxmlError> {
    let metadata = fs::symlink_metadata(source)?;
    if !metadata.file_type().is_file() {
        return Err(OoxmlError::invalid(
            "workbook package is not a regular file",
        ));
    }
    if metadata.len() > MAX_PACKAGE_BYTES {
        return Err(OoxmlError::limit_exceeded(
            "workbook package exceeded the size limit",
        ));
    }
    Ok(())
}

pub(super) fn open_archive<R: Read + Seek>(mut source: R) -> Result<ZipArchive<R>, OoxmlError> {
    let preflight = preflight_archive(&mut source)?;
    let mut archive = ZipArchive::new(source)?;
    if archive.len() != preflight.entry_count
        || archive.central_directory_start() != preflight.central_directory_start
    {
        return Err(OoxmlError::invalid("workbook ZIP metadata is invalid"));
    }
    validate_archive(&mut archive)?;
    Ok(archive)
}

pub(super) fn reject_zip64_or_malformed_extra<R: Read + Seek>(
    source: &mut R,
    start: u64,
    end: u64,
) -> Result<(), OoxmlError> {
    const EXTRA_HEADER_BYTES: u64 = 4;
    const ZIP64_EXTRA_HEADER_ID: u16 = 0x0001;

    let mut cursor = start;
    while cursor < end {
        let header_end = cursor
            .checked_add(EXTRA_HEADER_BYTES)
            .filter(|header_end| *header_end <= end)
            .ok_or_else(|| OoxmlError::invalid("workbook ZIP metadata is invalid"))?;
        source.seek(SeekFrom::Start(cursor))?;
        let mut header = [0_u8; EXTRA_HEADER_BYTES as usize];
        source.read_exact(&mut header)?;
        let header_id = u16::from_le_bytes([header[0], header[1]]);
        let data_len = u64::from(u16::from_le_bytes([header[2], header[3]]));
        if header_id == ZIP64_EXTRA_HEADER_ID {
            return Err(OoxmlError::invalid("workbook ZIP metadata is invalid"));
        }
        cursor = header_end
            .checked_add(data_len)
            .filter(|data_end| *data_end <= end)
            .ok_or_else(|| OoxmlError::invalid("workbook ZIP metadata is invalid"))?;
    }
    Ok(())
}

pub(super) fn preflight_archive<R: Read + Seek>(
    source: &mut R,
) -> Result<ZipPreflightFacts, OoxmlError> {
    const EOCD_MIN_BYTES: usize = 22;
    const EOCD_MAX_COMMENT_BYTES: usize = u16::MAX as usize;
    const EOCD_SIGNATURE: &[u8; 4] = b"PK\x05\x06";
    const CENTRAL_DIRECTORY_HEADER_BYTES: u64 = 46;
    const CENTRAL_DIRECTORY_HEADER_SIGNATURE: &[u8; 4] = b"PK\x01\x02";

    let source_len = source.seek(SeekFrom::End(0))?;
    if source_len == 0 {
        return Err(OoxmlError::invalid("workbook source is empty"));
    }
    if source_len > MAX_PACKAGE_BYTES {
        return Err(OoxmlError::limit_exceeded(
            "workbook source exceeded the size limit",
        ));
    }
    let tail_len = usize::try_from(source_len)
        .unwrap_or(usize::MAX)
        .min(EOCD_MIN_BYTES + EOCD_MAX_COMMENT_BYTES);
    source.seek(SeekFrom::Start(source_len - tail_len as u64))?;
    let mut tail = vec![0_u8; tail_len];
    source.read_exact(&mut tail)?;

    let eocd_offset = tail
        .windows(EOCD_SIGNATURE.len())
        .rposition(|window| window == EOCD_SIGNATURE)
        .ok_or_else(|| OoxmlError::invalid("workbook ZIP metadata is invalid"))?;
    let eocd = tail
        .get(eocd_offset..)
        .filter(|bytes| bytes.len() >= EOCD_MIN_BYTES)
        .ok_or_else(|| OoxmlError::invalid("workbook ZIP metadata is invalid"))?;
    let comment_len = usize::from(u16::from_le_bytes([eocd[20], eocd[21]]));
    if eocd_offset + EOCD_MIN_BYTES + comment_len != tail.len() {
        return Err(OoxmlError::invalid("workbook ZIP metadata is invalid"));
    }

    let disk_number = u16::from_le_bytes([eocd[4], eocd[5]]);
    let directory_disk = u16::from_le_bytes([eocd[6], eocd[7]]);
    let entries_on_disk = u16::from_le_bytes([eocd[8], eocd[9]]);
    let entry_count = u16::from_le_bytes([eocd[10], eocd[11]]);
    let directory_size = u32::from_le_bytes([eocd[12], eocd[13], eocd[14], eocd[15]]);
    let directory_offset = u32::from_le_bytes([eocd[16], eocd[17], eocd[18], eocd[19]]);
    if disk_number != 0 || directory_disk != 0 || entries_on_disk != entry_count {
        return Err(OoxmlError::invalid("workbook ZIP metadata is invalid"));
    }
    if usize::from(entry_count) > MAX_ENTRIES {
        return Err(OoxmlError::limit_exceeded(
            "workbook ZIP entry count is invalid",
        ));
    }
    if entry_count == u16::MAX || directory_size == u32::MAX || directory_offset == u32::MAX {
        return Err(OoxmlError::invalid("workbook ZIP metadata is invalid"));
    }
    let absolute_eocd_offset = source_len - tail_len as u64 + eocd_offset as u64;
    let directory_end = u64::from(directory_offset)
        .checked_add(u64::from(directory_size))
        .ok_or_else(|| OoxmlError::invalid("workbook ZIP metadata is invalid"))?;
    if directory_end > absolute_eocd_offset {
        return Err(OoxmlError::invalid("workbook ZIP metadata is invalid"));
    }
    if entry_count > 0 {
        let minimum_directory_size = u64::from(entry_count)
            .checked_mul(CENTRAL_DIRECTORY_HEADER_BYTES)
            .ok_or_else(|| OoxmlError::invalid("workbook ZIP metadata is invalid"))?;
        if u64::from(directory_size) < minimum_directory_size
            || directory_end != absolute_eocd_offset
            || u64::from(directory_offset) >= absolute_eocd_offset
        {
            return Err(OoxmlError::invalid("workbook ZIP metadata is invalid"));
        }
        let mut cursor = u64::from(directory_offset);
        for _ in 0..entry_count {
            let fixed_end = cursor
                .checked_add(CENTRAL_DIRECTORY_HEADER_BYTES)
                .filter(|end| *end <= directory_end)
                .ok_or_else(|| OoxmlError::invalid("workbook ZIP metadata is invalid"))?;
            source.seek(SeekFrom::Start(cursor))?;
            let mut header = [0_u8; CENTRAL_DIRECTORY_HEADER_BYTES as usize];
            source.read_exact(&mut header)?;
            if &header[..4] != CENTRAL_DIRECTORY_HEADER_SIGNATURE {
                return Err(OoxmlError::invalid("workbook ZIP metadata is invalid"));
            }
            let compressed_size =
                u32::from_le_bytes([header[20], header[21], header[22], header[23]]);
            let uncompressed_size =
                u32::from_le_bytes([header[24], header[25], header[26], header[27]]);
            let file_name_len = u64::from(u16::from_le_bytes([header[28], header[29]]));
            let extra_field_len = u64::from(u16::from_le_bytes([header[30], header[31]]));
            let comment_len = u64::from(u16::from_le_bytes([header[32], header[33]]));
            let disk_number_start = u16::from_le_bytes([header[34], header[35]]);
            let local_header_offset =
                u32::from_le_bytes([header[42], header[43], header[44], header[45]]);
            if disk_number_start != 0
                || compressed_size == u32::MAX
                || uncompressed_size == u32::MAX
                || local_header_offset == u32::MAX
            {
                return Err(OoxmlError::invalid("workbook ZIP metadata is invalid"));
            }
            let file_name_end = fixed_end
                .checked_add(file_name_len)
                .filter(|end| *end <= directory_end)
                .ok_or_else(|| OoxmlError::invalid("workbook ZIP metadata is invalid"))?;
            let extra_field_end = file_name_end
                .checked_add(extra_field_len)
                .filter(|end| *end <= directory_end)
                .ok_or_else(|| OoxmlError::invalid("workbook ZIP metadata is invalid"))?;
            cursor = extra_field_end
                .checked_add(comment_len)
                .filter(|end| *end <= directory_end)
                .ok_or_else(|| OoxmlError::invalid("workbook ZIP metadata is invalid"))?;
            reject_zip64_or_malformed_extra(source, file_name_end, extra_field_end)?;

            let local_header_offset = u64::from(local_header_offset);
            let local_fixed_end = local_header_offset
                .checked_add(30)
                .filter(|end| *end <= u64::from(directory_offset) && *end <= source_len)
                .ok_or_else(|| OoxmlError::invalid("workbook ZIP metadata is invalid"))?;
            source.seek(SeekFrom::Start(local_header_offset))?;
            let mut local_header = [0_u8; 30];
            source.read_exact(&mut local_header)?;
            if &local_header[..4] != b"PK\x03\x04" {
                return Err(OoxmlError::invalid("workbook ZIP metadata is invalid"));
            }
            let local_compressed_size = u32::from_le_bytes([
                local_header[18],
                local_header[19],
                local_header[20],
                local_header[21],
            ]);
            let local_uncompressed_size = u32::from_le_bytes([
                local_header[22],
                local_header[23],
                local_header[24],
                local_header[25],
            ]);
            let local_file_name_len =
                u64::from(u16::from_le_bytes([local_header[26], local_header[27]]));
            let local_extra_field_len =
                u64::from(u16::from_le_bytes([local_header[28], local_header[29]]));
            if local_compressed_size == u32::MAX
                || local_uncompressed_size == u32::MAX
                || local_file_name_len != file_name_len
            {
                return Err(OoxmlError::invalid("workbook ZIP metadata is invalid"));
            }
            let local_file_name_end = local_fixed_end
                .checked_add(local_file_name_len)
                .filter(|end| *end <= u64::from(directory_offset) && *end <= source_len)
                .ok_or_else(|| OoxmlError::invalid("workbook ZIP metadata is invalid"))?;
            let local_extra_field_end = local_file_name_end
                .checked_add(local_extra_field_len)
                .filter(|end| *end <= u64::from(directory_offset) && *end <= source_len)
                .ok_or_else(|| OoxmlError::invalid("workbook ZIP metadata is invalid"))?;
            reject_zip64_or_malformed_extra(source, local_file_name_end, local_extra_field_end)?;
        }
        if cursor != directory_end {
            return Err(OoxmlError::invalid("workbook ZIP metadata is invalid"));
        }
    }
    source.seek(SeekFrom::Start(0))?;
    Ok(ZipPreflightFacts {
        entry_count: usize::from(entry_count),
        central_directory_start: u64::from(directory_offset),
    })
}

pub(super) fn validate_archive<R: Read + Seek>(
    archive: &mut ZipArchive<R>,
) -> Result<(), OoxmlError> {
    if archive.is_empty() {
        return Err(OoxmlError::invalid("workbook ZIP entry count is invalid"));
    }
    if archive.len() > MAX_ENTRIES {
        return Err(OoxmlError::limit_exceeded(
            "workbook ZIP entry count is invalid",
        ));
    }
    let mut names = BTreeSet::new();
    let mut total = 0_u64;
    for index in 0..archive.len() {
        let entry = archive.by_index(index)?;
        if entry.size() > MAX_ENTRY_BYTES {
            return Err(OoxmlError::limit_exceeded(
                "workbook member exceeded the size limit",
            ));
        }
        if entry.encrypted()
            || entry.enclosed_name().is_none()
            || !names.insert(entry.name()?.into_owned())
        {
            return Err(OoxmlError::invalid("workbook ZIP entry is invalid"));
        }
        total = total.saturating_add(entry.size());
        if total > MAX_UNCOMPRESSED_BYTES {
            return Err(OoxmlError::limit_exceeded(
                "workbook uncompressed data exceeded the size limit",
            ));
        }
    }
    Ok(())
}

pub(super) fn read_entry<R: Read + Seek>(
    archive: &mut ZipArchive<R>,
    name: &str,
) -> Result<Vec<u8>, OoxmlError> {
    let mut entry = archive
        .by_name(name)
        .map_err(|_| OoxmlError::invalid(format!("workbook member not found: {name}")))?;
    if entry.size() > MAX_ENTRY_BYTES {
        return Err(OoxmlError::limit_exceeded(
            "workbook member exceeded the size limit",
        ));
    }
    let mut bytes = Vec::with_capacity(usize::try_from(entry.size()).unwrap_or(0));
    entry
        .by_ref()
        .take(MAX_ENTRY_BYTES + 1)
        .read_to_end(&mut bytes)?;
    if bytes.len() as u64 > MAX_ENTRY_BYTES {
        return Err(OoxmlError::limit_exceeded(
            "workbook member exceeded the size limit",
        ));
    }
    Ok(bytes)
}

pub(super) fn has_entry<R: Read + Seek>(
    archive: &mut ZipArchive<R>,
    name: &str,
) -> Result<bool, OoxmlError> {
    match archive.by_name(name) {
        Ok(_) => Ok(true),
        Err(zip::result::ZipError::FileNotFound) => Ok(false),
        Err(error) => Err(error.into()),
    }
}

fn copy_package_entries<R, W>(
    archive: &mut ZipArchive<R>,
    writer: &mut ZipWriter<W>,
    patched: &BTreeMap<String, Vec<u8>>,
) -> Result<(), OoxmlError>
where
    R: Read + Seek,
    W: Write + Seek,
{
    for index in 0..archive.len() {
        let entry = archive.by_index(index)?;
        let name = entry.name()?.into_owned();
        if let Some(bytes) = patched.get(&name) {
            let mut options = entry.options().into_full_options();
            if !entry.comment().is_empty() {
                options = options.with_file_comment(entry.comment());
            }
            writer.start_file(name, options)?;
            writer.write_all(bytes)?;
        } else {
            writer.raw_copy_file(entry)?;
        }
    }
    Ok(())
}

pub(super) fn write_package(
    archive: &mut ZipArchive<File>,
    output: File,
    patched: &BTreeMap<String, Vec<u8>>,
) -> Result<(), OoxmlError> {
    let archive_comment = archive.comment().to_vec();
    let mut writer = ZipWriter::new(output);
    writer.set_raw_comment(archive_comment.into_boxed_slice())?;
    copy_package_entries(archive, &mut writer, patched)?;
    let output = writer.finish()?;
    output.sync_all()?;
    Ok(())
}

pub(super) fn write_package_guarded<R, W>(
    archive: &mut ZipArchive<R>,
    output: &mut W,
    patched: &BTreeMap<String, Vec<u8>>,
) -> Result<(), OoxmlError>
where
    R: Read + Seek,
    W: Write + Seek,
{
    let archive_comment = archive.comment().to_vec();
    let limited = LimitedWriter::new(output, MAX_PACKAGE_BYTES)?;
    let mut writer = ZipWriter::new(limited);
    writer.set_raw_comment(archive_comment.into_boxed_slice())?;
    copy_package_entries(archive, &mut writer, patched)?;
    let mut output = writer.finish()?;
    output.flush()?;
    Ok(())
}

pub(super) struct LimitedWriter<'a, W> {
    pub(super) inner: &'a mut W,
    pub(super) position: u64,
    pub(super) limit: u64,
}

impl<'a, W: Seek> LimitedWriter<'a, W> {
    pub(super) fn new(inner: &'a mut W, limit: u64) -> io::Result<Self> {
        let position = inner.stream_position()?;
        if position > limit {
            return Err(io::Error::other("guarded output exceeded the size limit"));
        }
        Ok(Self {
            inner,
            position,
            limit,
        })
    }
}

impl<W: Write> Write for LimitedWriter<'_, W> {
    fn write(&mut self, buffer: &[u8]) -> io::Result<usize> {
        let requested = u64::try_from(buffer.len())
            .ok()
            .and_then(|length| self.position.checked_add(length))
            .filter(|end| *end <= self.limit)
            .ok_or_else(|| io::Error::other("guarded output exceeded the size limit"))?;
        let written = self.inner.write(buffer)?;
        self.position = self
            .position
            .checked_add(u64::try_from(written).unwrap_or(u64::MAX))
            .filter(|end| *end <= requested)
            .ok_or_else(|| io::Error::other("guarded output position is invalid"))?;
        Ok(written)
    }

    fn flush(&mut self) -> io::Result<()> {
        self.inner.flush()
    }
}

impl<W: Seek> Seek for LimitedWriter<'_, W> {
    fn seek(&mut self, position: SeekFrom) -> io::Result<u64> {
        let position = self.inner.seek(position)?;
        if position > self.limit {
            return Err(io::Error::other("guarded output exceeded the size limit"));
        }
        self.position = position;
        Ok(position)
    }
}

use super::*;

pub(super) fn resize_gray_to_fit(
    image: &GrayImage,
    max_width: usize,
    max_height: usize,
) -> Result<GrayImage, String> {
    if image.width == 0
        || image.height == 0
        || image.pixels.len() != image.width.saturating_mul(image.height)
        || max_width == 0
        || max_height == 0
    {
        return Err("thumbnail dimensions are invalid".to_string());
    }
    let width_scale = max_width as f64 / image.width as f64;
    let height_scale = max_height as f64 / image.height as f64;
    let scale = width_scale.min(height_scale).min(1.0);
    let width = ((image.width as f64 * scale).floor() as usize).max(1);
    let height = ((image.height as f64 * scale).floor() as usize).max(1);
    let mut pixels = Vec::with_capacity(width.saturating_mul(height));
    for y in 0..height {
        let source_y = y.saturating_mul(image.height) / height;
        for x in 0..width {
            let source_x = x.saturating_mul(image.width) / width;
            pixels.push(image.pixels[source_y * image.width + source_x]);
        }
    }
    Ok(GrayImage {
        width,
        height,
        pixels,
    })
}

pub(super) fn base64_encode(bytes: &[u8]) -> String {
    const TABLE: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut output = String::with_capacity(bytes.len().div_ceil(3) * 4);
    for chunk in bytes.chunks(3) {
        let first = chunk[0];
        let second = chunk.get(1).copied().unwrap_or(0);
        let third = chunk.get(2).copied().unwrap_or(0);
        output.push(TABLE[usize::from(first >> 2)] as char);
        output.push(TABLE[usize::from((first & 0x03) << 4 | second >> 4)] as char);
        output.push(if chunk.len() > 1 {
            TABLE[usize::from((second & 0x0f) << 2 | third >> 6)] as char
        } else {
            '='
        });
        output.push(if chunk.len() > 2 {
            TABLE[usize::from(third & 0x3f)] as char
        } else {
            '='
        });
    }
    output
}

pub fn image_pdf_from_png(input: &Path, output: &Path) -> Result<(), String> {
    use flate2::{Compression, write::ZlibEncoder};

    let image = read_png_gray(input)?;
    let mut encoder = ZlibEncoder::new(Vec::new(), Compression::default());
    encoder
        .write_all(&image.pixels)
        .map_err(|error| format!("image-only PDF compression: {error}"))?;
    let compressed = encoder
        .finish()
        .map_err(|error| format!("image-only PDF compression finish: {error}"))?;
    let content = b"q 612 0 0 792 0 0 cm /Im0 Do Q\n";
    let mut image_object = format!(
        "<< /Type /XObject /Subtype /Image /Width {} /Height {} /ColorSpace /DeviceGray /BitsPerComponent 8 /Filter /FlateDecode /Length {} >>\nstream\n",
        image.width,
        image.height,
        compressed.len()
    )
    .into_bytes();
    image_object.extend_from_slice(&compressed);
    image_object.extend_from_slice(b"\nendstream");
    let objects = [
        b"<< /Type /Catalog /Pages 2 0 R >>".to_vec(),
        b"<< /Type /Pages /Kids [3 0 R] /Count 1 >>".to_vec(),
        b"<< /Type /Page /Parent 2 0 R /MediaBox [0 0 612 792] /Resources << /XObject << /Im0 4 0 R >> >> /Contents 5 0 R >>".to_vec(),
        image_object,
        {
            let mut stream = format!("<< /Length {} >>\nstream\n", content.len()).into_bytes();
            stream.extend_from_slice(content);
            stream.extend_from_slice(b"endstream");
            stream
        },
    ];
    let mut bytes = b"%PDF-1.4\n%\xE2\xE3\xCF\xD3\n".to_vec();
    let mut offsets = Vec::new();
    for (index, object) in objects.iter().enumerate() {
        offsets.push(bytes.len());
        bytes.extend_from_slice(format!("{} 0 obj\n", index + 1).as_bytes());
        bytes.extend_from_slice(object);
        bytes.extend_from_slice(b"\nendobj\n");
    }
    let xref = bytes.len();
    bytes.extend_from_slice(
        format!("xref\n0 {}\n0000000000 65535 f \n", objects.len() + 1).as_bytes(),
    );
    for offset in offsets {
        bytes.extend_from_slice(format!("{offset:010} 00000 n \n").as_bytes());
    }
    bytes.extend_from_slice(
        format!(
            "trailer\n<< /Size {} /Root 1 0 R >>\nstartxref\n{xref}\n%%EOF\n",
            objects.len() + 1
        )
        .as_bytes(),
    );
    fs::write(output, bytes).map_err(|error| format!("image-only PDF write: {error}"))
}

#[cfg(not(target_os = "macos"))]
pub(super) fn page_text(_path: &Path, _page_index: usize) -> Result<String, OcrFailure> {
    Ok(String::new())
}

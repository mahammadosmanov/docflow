# Third-party notices

DocFlow's Rust and npm dependency graphs are audited from their locked metadata by
`node tools/license_audit.mjs`. The command writes a machine-readable report under
`artifacts/` and fails when a dependency has no declared license or matches the
project's rejected AGPL/Affero/commercial policy.

The bundled Tesseract executable, English model, native libraries, checksum file,
SPDX metadata, and upstream license/copyright texts are staged under
`src-tauri/resources/tesseract/`. Tauri copies that evidence into each application
package with the OCR runtime. `SHA256SUMS` covers every staged resource file.

This notice is an inventory aid, not a replacement for the license texts bundled
with their corresponding native components.

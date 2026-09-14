# ADR 0004: Bounded signed-page previews in the approved Plan

- Date: 2026-09-13
- Status: Accepted

## Context

Signed-page staging already pins the exact Worley scan page, archive candidate,
and review destination. Paths and page numbers alone do not let an operator catch
a visually incorrect signature page before Apply. Opening arbitrary local paths
from React would break the webview authority boundary, while an unbounded PDF or
image payload could exhaust memory on a large combined scan.

## Decision

During preview, the Rust authority may render inert grayscale PNG thumbnails for
the exact signed page and the first page of each archive candidate. Rendering is
read-only and supported only by the native macOS and Windows PDF adapters.

Each thumbnail is rendered at 144 DPI, reduced to at most 540 by 760 pixels, and
rejected above 768 KiB before base64 encoding. At most 20 RFI rows and 12 MiB of
encoded thumbnail data are admitted into `Plan.meta.pageComparisons`. The row ID
and opaque archive-candidate token bind every image to the same editor choice and
candidate steps already held by the approved Plan. The webview accepts only
bounded `data:image/png;base64` values with the PNG signature and receives no
local file URL or new file authority. Tauri's content security policy permits
`data:` only in `img-src`.

Missing rendering support, rendering failure, or a reached bound omits the image
and presents a manual-review message. It never changes candidate selection,
warnings, steps, Apply eligibility, or the exact mutation authority.

## Consequences

- Operators can compare the current archive page and proposed Worley page before
  applying a replacement.
- Changing an ambiguous archive selection changes the displayed archive image by
  the same opaque candidate token used to revise the Plan.
- Preview CPU and memory remain bounded independently of scan size and renderer
  success.
- Native Windows rendering still requires acceptance on the exact packaged
  Windows candidate; macOS verification cannot establish that result.

## Rollback

Removing the optional metadata and comparison component restores path-and-page
review. No stored document, journal, setting, or migration is involved because
the thumbnails exist only in the in-memory preview Plan.

## 2026-09-14 amendment

Operator testing found the original 240 by 320 thumbnails too small to compare
signatures and document identity. The revised per-image and total bounds above
remain finite while allowing an explicit enlarged comparison in the same inert
Plan metadata. Row count and authority boundaries are unchanged.

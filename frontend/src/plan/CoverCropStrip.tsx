import { type Strings } from "../i18n";
import { safePngThumbnail } from "./SignedPageComparison";

/** Small strip under the Preview: the printed RFI No. cell of each scanned cover. */
export function CoverCropStrip({
  rows,
  s,
}: {
  rows: Record<string, unknown>[];
  s: Strings;
}) {
  const crops = rows.flatMap((row) => {
    const image = safePngThumbnail(row.coverCrop);
    const code = typeof row.code === "string" ? row.code : "";
    return image ? [{ id: String(row._rowId ?? code), image, code }] : [];
  });
  if (crops.length === 0) return null;
  return (
    <section className="cover-crop-strip" aria-label={s.t("rfi.fastScan.coverCrops.title")}>
      <strong>{s.t("rfi.fastScan.coverCrops.title")}</strong>
      <div className="cover-crop-list">
        {crops.map((crop) => (
          <figure key={crop.id}>
            <img src={crop.image} alt={crop.code} />
            <figcaption>{crop.code}</figcaption>
          </figure>
        ))}
      </div>
    </section>
  );
}

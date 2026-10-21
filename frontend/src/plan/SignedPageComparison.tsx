import {
  ArrowLeft,
  ArrowRight,
} from "lucide-react";
import {
  useEffect,
  useRef,
  useState,
} from "react";
import {
  Modal,
  Notice,
} from "../components/ui";
import {
  type Strings,
} from "../i18n";
import {
  type Plan,
} from "../types";

export function safePngThumbnail(value: unknown): string | null {
  const prefix = "data:image/png;base64,";
  const pngSignature = "iVBORw0KGgo";
  const payload = typeof value === "string" ? value.slice(prefix.length) : "";
  if (
    typeof value !== "string" ||
    !value.startsWith(prefix) ||
    value.length > 1_100_000 ||
    !payload.startsWith(pngSignature) ||
    payload.length % 4 !== 0 ||
    !/^[A-Za-z0-9+/]*={0,2}$/.test(payload)
  ) {
    return null;
  }
  return value;
}

export function SignedPageComparison({
  plan,
  editorRows,
  focusCode,
  s,
}: {
  plan: Plan;
  editorRows: Record<string, unknown>[];
  focusCode: string;
  s: Strings;
}) {
  const sectionRef = useRef<HTMLElement>(null);
  const raw =
    plan.meta?.pageComparisons && typeof plan.meta.pageComparisons === "object"
      ? (plan.meta.pageComparisons as Record<string, unknown>)
      : {};
  const available = editorRows.filter((row) => {
    const rowId = typeof row._rowId === "string" ? row._rowId : "";
    const comparison = raw[rowId];
    if (!comparison || typeof comparison !== "object") return false;
    const record = comparison as Record<string, unknown>;
    const archives =
      record.archives && typeof record.archives === "object"
        ? Object.values(record.archives as Record<string, unknown>)
        : [];
    return Boolean(
      safePngThumbnail(record.signed) || archives.some(safePngThumbnail),
    );
  });
  const availableIds = available.map((row) => String(row._rowId));
  const [selectedId, setSelectedId] = useState(availableIds[0] ?? "");
  const [expanded, setExpanded] = useState<"archive" | "signed" | null>(null);
  const availableSignature = availableIds.join("\u0000");
  useEffect(() => {
    if (!availableIds.includes(selectedId)) {
      setSelectedId(availableIds[0] ?? "");
    }
  }, [availableSignature, selectedId]);
  useEffect(() => {
    if (!focusCode) return;
    const row = available.find((candidate) => candidate.code === focusCode);
    if (!row) return;
    setSelectedId(String(row._rowId));
    sectionRef.current?.scrollIntoView?.({ block: "nearest" });
  }, [availableSignature, focusCode]);
  const omitted =
    typeof plan.meta?.pagePreviewOmitted === "number"
      ? plan.meta.pagePreviewOmitted
      : 0;
  if (available.length === 0) {
    return omitted > 0 ? (
      <Notice kind="warning">{s.t("rfi.signed.previewUnavailable")}</Notice>
    ) : null;
  }
  const selected =
    available.find((row) => row._rowId === selectedId) ?? available[0];
  const comparison = raw[String(selected._rowId)] as Record<string, unknown>;
  const archives =
    comparison.archives && typeof comparison.archives === "object"
      ? (comparison.archives as Record<string, unknown>)
      : {};
  const archiveToken =
    typeof selected.archiveFile === "string" ? selected.archiveFile : "";
  const archivePreview = safePngThumbnail(archives[archiveToken]);
  const signedPreview = safePngThumbnail(comparison.signed);
  const code = typeof selected.code === "string" ? selected.code : selectedId;
  const signedFile =
    typeof selected.signedFile === "string" ? selected.signedFile : "";
  const choiceLabels =
    plan.meta?.editor && typeof plan.meta.editor === "object"
      ? ((plan.meta.editor as Record<string, unknown>).choiceLabels as
          | Record<string, unknown>
          | undefined)
      : undefined;
  const archiveLabels =
    choiceLabels?.archiveFile && typeof choiceLabels.archiveFile === "object"
      ? (choiceLabels.archiveFile as Record<string, unknown>)
      : {};
  const archiveFile =
    typeof archiveLabels[archiveToken] === "string"
      ? String(archiveLabels[archiveToken])
      : "";
  const selectedIndex = available.findIndex(
    (row) => String(row._rowId) === String(selected._rowId),
  );
  const selectOffset = (offset: number) => {
    const target = available[selectedIndex + offset];
    if (target) setSelectedId(String(target._rowId));
  };
  return (
    <>
      <section
        id="signed-page-comparison"
        ref={sectionRef}
        className="signed-page-comparison"
        aria-labelledby="signed-page-comparison-title"
      >
        <div className="signed-page-comparison-heading">
          <div>
            <strong id="signed-page-comparison-title">
              {s.t("rfi.signed.previewTitle")}
            </strong>
            <span>{s.t("rfi.signed.previewHelp")}</span>
          </div>
          <div className="signed-page-comparison-selector">
            <button
              type="button"
              aria-label={s.t("rfi.signed.previewPrevious")}
              disabled={selectedIndex <= 0}
              onClick={() => selectOffset(-1)}
            >
              <ArrowLeft aria-hidden="true" />
            </button>
            {available.length > 1 ? (
              <select
                aria-label={s.t("rfi.signed.previewRfi")}
                value={String(selected._rowId)}
                onChange={(event) => setSelectedId(event.target.value)}
              >
                {available.map((row) => (
                  <option key={String(row._rowId)} value={String(row._rowId)}>
                    {typeof row.code === "string"
                      ? row.code
                      : String(row._rowId)}
                  </option>
                ))}
              </select>
            ) : (
              <span className="signed-page-comparison-code">{code}</span>
            )}
            <button
              type="button"
              aria-label={s.t("rfi.signed.previewNext")}
              disabled={selectedIndex >= available.length - 1}
              onClick={() => selectOffset(1)}
            >
              <ArrowRight aria-hidden="true" />
            </button>
          </div>
        </div>
        <div className="signed-page-change-summary">
          <span>{s.t("rfi.signed.previewArchive")}</span>
          <ArrowRight aria-hidden="true" />
          <strong>{s.t("rfi.signed.previewSigned")}</strong>
          <small>{s.t("rfi.signed.previewOnlyPageOne")}</small>
        </div>
        <div className="signed-page-comparison-grid">
          <figure>
            <figcaption>
              <span>{s.t("rfi.signed.previewBefore")}</span>
              <strong>{s.t("rfi.signed.previewArchive")}</strong>
              {archiveFile && <small title={archiveFile}>{archiveFile}</small>}
            </figcaption>
            {archivePreview ? (
              <button
                type="button"
                className="signed-page-preview-button"
                aria-label={`${s.t("rfi.signed.previewArchiveAlt", { code })} · ${s.t("rfi.signed.previewEnlarge")}`}
                onClick={() => setExpanded("archive")}
              >
                <img
                  src={archivePreview}
                  alt={s.t("rfi.signed.previewArchiveAlt", { code })}
                />
                <span>{s.t("rfi.signed.previewEnlarge")}</span>
              </button>
            ) : (
              <div className="signed-page-preview-empty">
                {archiveToken
                  ? s.t("rfi.signed.previewUnavailable")
                  : s.t("rfi.signed.previewChooseArchive")}
              </div>
            )}
          </figure>
          <figure>
            <figcaption>
              <span>{s.t("rfi.signed.previewAfter")}</span>
              <strong>{s.t("rfi.signed.previewSigned")}</strong>
              {signedFile && <small title={signedFile}>{signedFile}</small>}
            </figcaption>
            {signedPreview ? (
              <button
                type="button"
                className="signed-page-preview-button"
                aria-label={`${s.t("rfi.signed.previewSignedAlt", { code })} · ${s.t("rfi.signed.previewEnlarge")}`}
                onClick={() => setExpanded("signed")}
              >
                <img
                  src={signedPreview}
                  alt={s.t("rfi.signed.previewSignedAlt", { code })}
                />
                <span>{s.t("rfi.signed.previewEnlarge")}</span>
              </button>
            ) : (
              <div className="signed-page-preview-empty">
                {s.t("rfi.signed.previewUnavailable")}
              </div>
            )}
          </figure>
        </div>
        {omitted > 0 && (
          <small>{s.t("rfi.signed.previewOmitted", { count: omitted })}</small>
        )}
      </section>
      {expanded && (expanded === "archive" ? archivePreview : signedPreview) && (
        <Modal
          title={`${code} · ${s.t(
            expanded === "archive"
              ? "rfi.signed.previewArchive"
              : "rfi.signed.previewSigned",
          )}`}
          onClose={() => setExpanded(null)}
        >
          <div className="signed-page-preview-modal">
            <img
              src={expanded === "archive" ? archivePreview! : signedPreview!}
              alt={s.t(
                expanded === "archive"
                  ? "rfi.signed.previewArchiveAlt"
                  : "rfi.signed.previewSignedAlt",
                { code },
              )}
            />
          </div>
        </Modal>
      )}
    </>
  );
}

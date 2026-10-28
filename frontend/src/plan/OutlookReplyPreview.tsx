import {
  type Strings,
} from "../i18n";
import {
  type outlookReplyPreview,
} from "../screens/ResultView";

export function OutlookReplyPreview({
  preview,
  s,
}: {
  preview: NonNullable<ReturnType<typeof outlookReplyPreview>>;
  s: Strings;
}) {
  return (
    <section
      className="outlook-reply-preview"
      aria-labelledby="outlook-reply-preview-title"
    >
      <h3 id="outlook-reply-preview-title">
        {s.t("outlookDraft.previewTitle")}
      </h3>
      <div>
        <strong>{s.t("outlookDraft.subject")}</strong>
        <pre>{preview.subject}</pre>
      </div>
      <div>
        <strong>{s.t("outlookDraft.plainText")}</strong>
        <pre>{preview.plainText}</pre>
      </div>
      <div>
        <strong>{s.t("outlookDraft.html")}</strong>
        <pre>{preview.html}</pre>
      </div>
    </section>
  );
}

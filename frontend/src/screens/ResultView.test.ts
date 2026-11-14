import { createElement } from "react";
import { renderToStaticMarkup } from "react-dom/server";
import { describe, expect, it } from "vitest";

import { makeStrings } from "../i18n";
import type { OperationResult } from "../types";
import { outlookReplyPreview, ResultView } from "./ResultView";

describe("outlookReplyPreview", () => {
  it("keeps only the subject, plain text, and HTML of a complete preview", () => {
    expect(
      outlookReplyPreview({
        subject: "Re: RFI",
        plainText: "ok",
        html: "<p>ok</p>",
        recipients: ["x@example.com"],
      }),
    ).toEqual({ subject: "Re: RFI", plainText: "ok", html: "<p>ok</p>" });
  });

  it("rejects non-objects and previews missing a string field", () => {
    expect(outlookReplyPreview(null)).toBeNull();
    expect(outlookReplyPreview("text")).toBeNull();
    expect(outlookReplyPreview({ subject: "s", plainText: "p" })).toBeNull();
    expect(
      outlookReplyPreview({ subject: "s", plainText: 1, html: "<p/>" }),
    ).toBeNull();
  });
});

describe("ResultView per-file results", () => {
  const s = makeStrings("en");
  const render = (meta: Record<string, unknown>) =>
    renderToStaticMarkup(
      createElement(ResultView, {
        result: {
          applied: 1,
          failed: 1,
          journalId: "",
          errors: [],
          meta,
        } as unknown as OperationResult,
        s,
        onDone: () => {},
        onUndo: () => {},
      }),
    );

  it("renders each file name with its translated status", () => {
    const html = render({
      jobResults: [
        { file: "a.pdf", status: "completed", copy: 1 },
        { file: "b.pdf", status: "failed", copy: 1 },
      ],
    });
    expect(html).toContain("a.pdf");
    expect(html).toContain("b.pdf");
    expect(html).toContain(s.t("print.status.completed"));
    expect(html).toContain(s.t("print.status.failed"));
  });

  it("renders no file table when the result carries no jobResults", () => {
    expect(render({})).not.toContain("<table");
  });
});

import { describe, expect, it } from "vitest";

import { outlookReplyPreview } from "./ResultView";

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

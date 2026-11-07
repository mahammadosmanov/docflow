import { cleanup, fireEvent, render, screen } from "@testing-library/react";
import { afterEach, describe, expect, it, vi } from "vitest";

import { makeStrings } from "../i18n";
import type { Bridge } from "../lib/bridge";
import type { TemplateDraft } from "../types";
import { TemplateDocument, fieldKey } from "./TemplateDocument";

const s = makeStrings("en");

const draft = {
  versionId: "v",
  placeholders: [],
  fieldMap: { schema: 1, fileName: "{{rfiNo}}", fields: [], presets: [] },
} as unknown as TemplateDraft;

describe("TemplateDocument", () => {
  afterEach(cleanup);

  it("makes ASCII camelCase keys that never collide", () => {
    expect(fieldKey("Date of issue", [])).toBe("dateOfIssue");
    expect(fieldKey("İşlem tarihi", [])).toBe("islemTarihi");
    expect(fieldKey("2.5 stage", [])).toBe("field25Stage");
    expect(fieldKey("Date", ["date", "date2"])).toBe("date3");
  });

  it("turns selected text into a field from the RFI Log", async () => {
    const call = vi.fn(async (method: string) =>
      method === "templates/blocks"
        ? { blocks: [{ part: "word/document.xml", index: 3, text: "Date: 07.11.2026" }] }
        : draft,
    );
    const beforeEdit = vi.fn(async () => {});
    const onEdited = vi.fn();
    render(
      <TemplateDocument
        bridge={{ call } as unknown as Bridge}
        s={s}
        draft={draft}
        fields={[]}
        beforeEdit={beforeEdit}
        onEdited={onEdited}
      />,
    );
    const block = await screen.findByText("Date: 07.11.2026");
    const range = document.createRange();
    range.setStart(block.firstChild!, 6);
    range.setEnd(block.firstChild!, 16);
    window.getSelection()!.removeAllRanges();
    window.getSelection()!.addRange(range);
    fireEvent.mouseUp(block);

    fireEvent.click(await screen.findByRole("button", { name: /Make field/ }));
    fireEvent.change(screen.getByLabelText("Value comes from"), {
      target: { value: "dateOfIssue" },
    });
    fireEvent.click(screen.getByRole("button", { name: /Apply to template/ }));
    await vi.waitFor(() => expect(onEdited).toHaveBeenCalled());
    expect(beforeEdit).toHaveBeenCalled();
    expect(call).toHaveBeenCalledWith("templates/editText", {
      versionId: "v",
      part: "word/document.xml",
      index: 3,
      start: 6,
      end: 16,
      expected: "07.11.2026",
      replacement: "",
      field: { key: "dateOfIssue", label: "Date of issue", value: "{dateOfIssue}" },
    });
  });
});

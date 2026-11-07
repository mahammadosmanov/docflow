import { cleanup, fireEvent, render, screen } from "@testing-library/react";
import { afterEach, describe, expect, it, vi } from "vitest";

import { makeStrings } from "../i18n";
import type { Bridge } from "../lib/bridge";
import type { TemplateDraft } from "../types";
import { TemplateEditor } from "./TemplateEditor";

const s = makeStrings("en");

const draft: TemplateDraft = {
  templateId: "t",
  versionId: "v",
  version: 2,
  code: "RFI-COVER-ELE",
  title: "RFI Cover (ELE)",
  discipline: "ELE",
  fileKind: "docx",
  placeholders: ["rfiNo", "issuer"],
  fieldMap: {
    schema: 1,
    fileName: "{{rfiNo}}",
    fields: [
      { key: "rfiNo", label: "RFI No", value: "{rfiNo}", options: [], multiline: false },
      { key: "issuer", label: "Issuer", value: "", options: [], multiline: false },
      { key: "old", label: "Old", value: "", options: [], multiline: false },
    ],
    presets: [],
  },
};

describe("TemplateEditor", () => {
  afterEach(cleanup);

  it("saves choices and presets, then publishes", async () => {
    const call = vi.fn(async (method: string, params?: { fieldMap?: unknown }) =>
      method === "templates/saveDraft" ? { ...draft, fieldMap: params?.fieldMap } : { templates: [] },
    );
    const onClose = vi.fn();
    render(
      <TemplateEditor bridge={{ call } as unknown as Bridge} s={s} draft={draft} onClose={onClose} />,
    );

    expect(screen.getByText("{{old}}")).toBeInTheDocument();
    fireEvent.click(screen.getByRole("button", { name: /Remove/ }));
    expect(screen.queryByText("{{old}}")).not.toBeInTheDocument();

    const choices = screen.getAllByLabelText("Choices (one per line)");
    fireEvent.change(choices[1], { target: { value: "Mahammad Osmanov\n\nEmin Dashdamirov " } });
    fireEvent.click(screen.getByRole("button", { name: /Add preset/ }));
    fireEvent.change(screen.getByLabelText("Preset name"), { target: { value: "ITP 5 · 2.5, 2.6" } });
    fireEvent.change(screen.getByLabelText("Issuer"), { target: { value: "Emin Dashdamirov" } });

    fireEvent.click(screen.getByRole("button", { name: /Publish to everyone/ }));
    await vi.waitFor(() => expect(onClose).toHaveBeenCalled());

    const saved = call.mock.calls.find(([method]) => method === "templates/saveDraft")?.[1] as {
      fieldMap: TemplateDraft["fieldMap"];
    };
    expect(saved.fieldMap.fields.map((field) => field.key)).toEqual(["rfiNo", "issuer"]);
    expect(saved.fieldMap.fields[1].options).toEqual(["Mahammad Osmanov", "Emin Dashdamirov"]);
    expect(saved.fieldMap.presets).toEqual([
      { name: "ITP 5 · 2.5, 2.6", values: { issuer: "Emin Dashdamirov" } },
    ]);
    expect(call).toHaveBeenLastCalledWith("templates/publish", { versionId: "v" });
  });
});

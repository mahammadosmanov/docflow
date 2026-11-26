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
  category: "Covers",
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

  it("sets a field type and format, and clears lines and choices for a typed field", async () => {
    const call = vi.fn(async (method: string, params?: { fieldMap?: unknown }) =>
      method === "templates/saveDraft" ? { ...draft, fieldMap: params?.fieldMap } : { templates: [] },
    );
    render(
      <TemplateEditor bridge={{ call } as unknown as Bridge} s={s} draft={draft} onClose={vi.fn()} />,
    );
    const types = screen.getAllByLabelText("Type");
    expect(types[0]).toHaveValue("text");
    expect(screen.queryByLabelText(/^Format/)).not.toBeInTheDocument();
    fireEvent.change(types[0], { target: { value: "date" } });
    fireEvent.change(screen.getByLabelText(/^Format/), { target: { value: "yyyy-MM-dd" } });
    expect(Array.from((types[0] as HTMLSelectElement).options).map((option) => option.value)).toContain("time");
    fireEvent.change(types[1], { target: { value: "checkbox" } });
    expect(screen.getAllByLabelText(/^Format/)[1]).toHaveAttribute("placeholder", "☒|☐");
    fireEvent.click(screen.getByRole("button", { name: /Save draft/ }));
    await vi.waitFor(() => expect(call).toHaveBeenCalledWith("templates/saveDraft", expect.anything()));
    const saved = call.mock.calls.find(([method]) => method === "templates/saveDraft")?.[1] as {
      fieldMap: TemplateDraft["fieldMap"];
    };
    expect(saved.fieldMap.fields[0]).toMatchObject({ kind: "date", format: "yyyy-MM-dd" });
    expect(saved.fieldMap.fields[1]).toMatchObject({ kind: "checkbox", options: [], multiline: false });
  });

  it("builds a computed field from other fields", async () => {
    const call = vi.fn(async (method: string, params?: { fieldMap?: unknown }) =>
      method === "templates/saveDraft" ? { ...draft, fieldMap: params?.fieldMap } : { templates: [] },
    );
    render(
      <TemplateEditor bridge={{ call } as unknown as Bridge} s={s} draft={draft} onClose={vi.fn()} />,
    );
    const types = screen.getAllByLabelText("Type");
    fireEvent.change(types[0], { target: { value: "number" } });
    fireEvent.change(types[1], { target: { value: "computed" } });
    expect(screen.queryByRole("checkbox", { name: "Issuer" })).not.toBeInTheDocument();
    fireEvent.click(screen.getByRole("checkbox", { name: "RFI No" }));
    fireEvent.change(screen.getByLabelText("Operation"), { target: { value: "offset" } });
    fireEvent.change(screen.getByLabelText(/^Number to add/), { target: { value: "-0.6" } });
    fireEvent.change(screen.getByLabelText(/^Decimals/), { target: { value: "1" } });
    fireEvent.click(screen.getByRole("button", { name: /Save draft/ }));
    await vi.waitFor(() => expect(call).toHaveBeenCalledWith("templates/saveDraft", expect.anything()));
    const saved = call.mock.calls.find(([method]) => method === "templates/saveDraft")?.[1] as {
      fieldMap: TemplateDraft["fieldMap"];
    };
    expect(saved.fieldMap.fields[1]).toMatchObject({
      kind: "computed",
      compute: { op: "offset", inputs: ["rfiNo"], by: "-0.6", decimals: 1 },
    });
  });

  it("renames with the discipline prefix and deletes for everyone", async () => {
    const call = vi.fn(async (method: string, params?: Record<string, unknown>) =>
      method === "templates/saveDraft" ? { ...draft, code: params?.code, title: params?.title } : { templates: [] },
    );
    const onClose = vi.fn();
    render(
      <TemplateEditor bridge={{ call } as unknown as Bridge} s={s} draft={draft} onClose={onClose} />,
    );
    fireEvent.change(screen.getByLabelText("Code"), { target: { value: "RFI-COVER" } });
    fireEvent.change(screen.getByLabelText("Discipline"), { target: { value: "MEC" } });
    expect(screen.getByLabelText(/^Category/)).toHaveValue("Covers");
    fireEvent.change(screen.getByLabelText(/^Category/), { target: { value: " Covers / RFI " } });
    expect(screen.getByLabelText("Code")).toHaveValue("MEC-RFI-COVER");
    expect(screen.getByLabelText("Name")).toHaveValue("MEC-RFI Cover (ELE)");
    fireEvent.click(screen.getByRole("button", { name: /Save draft/ }));
    await vi.waitFor(() =>
      expect(call).toHaveBeenCalledWith(
        "templates/saveDraft",
        expect.objectContaining({
          code: "MEC-RFI-COVER",
          discipline: "MEC",
          category: "Covers / RFI",
        }),
      ),
    );

    expect(await screen.findByText("Draft saved.")).toBeInTheDocument();
    fireEvent.click(screen.getByRole("button", { name: /Delete template/ }));
    const buttons = screen.getAllByRole("button", { name: /Delete template/ });
    fireEvent.click(buttons[buttons.length - 1]);
    await vi.waitFor(() =>
      expect(call).toHaveBeenCalledWith("templates/archive", { code: "MEC-RFI-COVER" }),
    );
    expect(onClose).toHaveBeenCalled();
  });

  const anchored: TemplateDraft = {
    ...draft,
    placeholders: [],
    fieldMap: {
      schema: 1,
      fileName: "{{rfiNo}}",
      fields: [
        { key: "pour", label: "Pour", value: "", options: [], multiline: false, anchor: { table: 0, row: 0, cell: 1 } },
        { key: "rfi", label: "RFI", value: "", options: [], multiline: false, anchor: { table: 0, row: 1, cell: 1 } },
      ],
      presets: [],
    },
  };

  it("holds publishing until every marked cell is resolved after a file replacement", async () => {
    const replaced: TemplateDraft = {
      ...anchored,
      checks: [
        { key: "pour", status: "moved", movedTo: { table: 0, row: 3, cell: 1 } },
        { key: "rfi", status: "missing", movedTo: null },
      ],
    };
    const call = vi.fn(async (method: string, params?: { fieldMap?: unknown }) => {
      if (method === "templates/replaceFile") return replaced;
      if (method === "templates/saveDraft") return { ...anchored, fieldMap: params?.fieldMap };
      return { templates: [] };
    });
    render(
      <TemplateEditor bridge={{ call } as unknown as Bridge} s={s} draft={anchored} onClose={vi.fn()} />,
    );
    fireEvent.click(screen.getByRole("button", { name: /Replace file/ }));
    expect(await screen.findByText("Pour moved to Table 1, row 4, cell 2.")).toBeInTheDocument();
    expect(screen.getByText("RFI was not found in the new file.")).toBeInTheDocument();
    expect(screen.getByRole("button", { name: /Publish to everyone/ })).toBeDisabled();
    expect(screen.getByText("Resolve the file check before publishing.")).toBeInTheDocument();

    fireEvent.click(screen.getByRole("button", { name: "Move it" }));
    fireEvent.click(screen.getByRole("button", { name: "Keep here" }));
    expect(screen.queryByText("RFI was not found in the new file.")).not.toBeInTheDocument();
    expect(screen.getByRole("button", { name: /Publish to everyone/ })).toBeEnabled();

    fireEvent.click(screen.getByRole("button", { name: /Save draft/ }));
    await vi.waitFor(() => expect(call).toHaveBeenCalledWith("templates/saveDraft", expect.anything()));
    const saved = call.mock.calls.find(([method]) => method === "templates/saveDraft")?.[1] as {
      fieldMap: TemplateDraft["fieldMap"];
    };
    expect(saved.fieldMap.fields[0].anchor).toEqual({ table: 0, row: 3, cell: 1 });
  });

  it("lists versions and restores an old one as the draft", async () => {
    const call = vi.fn(async (method: string) => {
      if (method === "templates/versions") {
        return {
          versions: [
            { versionId: "v3", version: 3, status: "draft", publishedAt: null, note: "" },
            { versionId: "v2", version: 2, status: "published", publishedAt: "2026-11-20T00:00:00Z", note: "" },
            { versionId: "v1", version: 1, status: "retired", publishedAt: "2026-11-01T00:00:00Z", note: "" },
          ],
        };
      }
      if (method === "templates/restoreVersion") return { ...draft, version: 4 };
      return { templates: [] };
    });
    render(
      <TemplateEditor bridge={{ call } as unknown as Bridge} s={s} draft={draft} onClose={vi.fn()} />,
    );
    fireEvent.click(screen.getByRole("button", { name: /Version history/ }));
    expect(await screen.findByText("Versions of RFI-COVER-ELE")).toBeInTheDocument();
    expect(call).toHaveBeenCalledWith("templates/versions", { code: "RFI-COVER-ELE" });
    expect(screen.getAllByRole("button", { name: "Restore as draft" })).toHaveLength(2);
    expect(screen.getByText(/replaces the draft you have open/)).toBeInTheDocument();
    fireEvent.click(screen.getAllByRole("button", { name: "Restore as draft" })[1]);
    await vi.waitFor(() =>
      expect(call).toHaveBeenCalledWith("templates/restoreVersion", {
        code: "RFI-COVER-ELE",
        versionId: "v1",
      }),
    );
    expect(await screen.findByText(/Draft v4/)).toBeInTheDocument();
  });
});

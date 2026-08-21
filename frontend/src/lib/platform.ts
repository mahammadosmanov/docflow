import { open } from "@tauri-apps/plugin-dialog";
import { openPath } from "@tauri-apps/plugin-opener";

function isTauri(): boolean {
  return Boolean(window.__TAURI_INTERNALS__);
}

export async function chooseFolder(): Promise<string | null> {
  if (!isTauri()) return "/Users/demo/Documents/DocFlow";
  const selected = await open({ directory: true, multiple: false });
  return typeof selected === "string" ? selected : null;
}

export async function chooseFile(): Promise<string | null> {
  if (!isTauri()) return "/Users/demo/Documents/file.xlsx";
  const selected = await open({
    directory: false,
    multiple: false,
  });
  return typeof selected === "string" ? selected : null;
}

export async function chooseFiles(): Promise<string[]> {
  if (!isTauri()) {
    return [
      "/Users/demo/Documents/first.pdf",
      "/Users/demo/Documents/second.docx",
    ];
  }
  const selected = await open({ directory: false, multiple: true });
  if (Array.isArray(selected)) return selected;
  return typeof selected === "string" ? [selected] : [];
}

export async function revealPath(path: string): Promise<void> {
  if (!isTauri()) return;
  await openPath(path);
}

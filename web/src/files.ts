/**
 * Browser replacements for the desktop file dialogs:
 *
 * * open `.290` (workbook JSON) or `.csv` via `<input type="file">`
 * * download the workbook as `.290` or the current sheet as `.csv`
 *
 * Serialization itself is done by the Rust core; this file only moves text
 * in and out of the browser.
 */

export type FileKind = "290" | "csv";

/** Decides how to import a file from its name (like the desktop "Open"). */
export function classifyFileName(name: string): FileKind | null {
  const lower = name.toLowerCase();
  if (lower.endsWith(".290") || lower.endsWith(".json")) return "290";
  if (lower.endsWith(".csv")) return "csv";
  return null;
}

/** `report` -> `report.290`, `report.290` -> unchanged (desktop appended the suffix). */
export function withExtension(name: string, ext: FileKind): string {
  const trimmed = name.trim() || "sheet";
  return trimmed.toLowerCase().endsWith(`.${ext}`) ? trimmed : `${trimmed}.${ext}`;
}

/** A sheet name that is safe as a file name. */
export function safeFileName(name: string): string {
  return name.replace(/[\\/:*?"<>|]+/g, "_").trim() || "sheet";
}

export function readFileText(file: Blob): Promise<string> {
  return file.text();
}

/** Triggers a browser download of `text`. */
export function downloadText(filename: string, text: string, mime = "application/json"): void {
  const blob = new Blob([text], { type: mime });
  const url = URL.createObjectURL(blob);
  const a = document.createElement("a");
  a.href = url;
  a.download = filename;
  document.body.appendChild(a);
  a.click();
  a.remove();
  setTimeout(() => URL.revokeObjectURL(url), 1000);
}

/** Opens the native file picker and resolves with the chosen file (or null). */
export function pickFile(accept: string): Promise<File | null> {
  return new Promise((resolve) => {
    const input = document.createElement("input");
    input.type = "file";
    input.accept = accept;
    input.style.display = "none";
    input.addEventListener("change", () => {
      resolve(input.files?.[0] ?? null);
      input.remove();
    });
    input.addEventListener("cancel", () => {
      resolve(null);
      input.remove();
    });
    document.body.appendChild(input);
    input.click();
  });
}

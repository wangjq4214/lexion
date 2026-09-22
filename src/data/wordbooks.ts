import { invoke } from "@tauri-apps/api/core";
import { open } from "@tauri-apps/plugin-dialog";
import type { WordEntry } from "../domain/word";

export type WordbookSummary = {
  id: number;
  name: string;
  entryCount: number;
};

export type ImportWordbookRequest = {
  path: string;
  name: string;
  replaceExisting: boolean;
};

export type ImportWordbookResult = {
  wordbook: WordbookSummary;
};

export type WordbookErrorKind =
  | "conflict"
  | "validation"
  | "database"
  | "unknown";

export class WordbookError extends Error {
  readonly kind: WordbookErrorKind;

  constructor(kind: WordbookErrorKind, message: string) {
    super(message);
    this.name = "WordbookError";
    this.kind = kind;
  }
}

export type WordbookService = {
  pickWorkbookFile(): Promise<string | null>;
  listWordbooks(): Promise<WordbookSummary[]>;
  importWordbook(request: ImportWordbookRequest): Promise<ImportWordbookResult>;
  sampleWordbook(wordbookId: number, limit: number): Promise<WordEntry[]>;
};

function commandError(error: unknown): WordbookError {
  if (typeof error === "object" && error !== null && "kind" in error) {
    const value = error as { kind?: unknown; message?: unknown };
    const kind = typeof value.kind === "string" ? value.kind : "unknown";
    if (["conflict", "validation", "database"].includes(kind)) {
      return new WordbookError(
        kind as WordbookErrorKind,
        typeof value.message === "string" ? value.message : "操作失败",
      );
    }
  }
  return new WordbookError(
    "unknown",
    error instanceof Error ? error.message : String(error),
  );
}

export const tauriWordbookService: WordbookService = {
  async pickWorkbookFile() {
    const selected = await open({
      multiple: false,
      directory: false,
      filters: [{ name: "Excel 工作簿", extensions: ["xlsx", "xls"] }],
    });
    return typeof selected === "string" ? selected : null;
  },

  listWordbooks() {
    return invoke<WordbookSummary[]>("list_wordbooks").catch((error) => {
      throw commandError(error);
    });
  },

  importWordbook(request) {
    return invoke<ImportWordbookResult>("import_wordbook", request).catch(
      (error) => {
        throw commandError(error);
      },
    );
  },

  sampleWordbook(wordbookId, limit) {
    return invoke<WordEntry[]>("sample_wordbook", { wordbookId, limit }).catch(
      (error) => {
        throw commandError(error);
      },
    );
  },
};

export function defaultWordbookName(path: string): string {
  const parts = path.split(/[\\/]/);
  const fileName = parts[parts.length - 1] ?? path;
  return fileName.replace(/\.(xlsx|xls)$/i, "");
}

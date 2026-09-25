import { invoke } from "@tauri-apps/api/core";
import { open } from "@tauri-apps/plugin-dialog";
import type { WordEntry } from "../domain/word";

export type MistakeEntry = WordEntry & { errorCount: number };
export type ScheduledQuestion = {
  reviewId: number;
  entry: WordEntry;
  direction: "zh-to-en" | "en-to-zh";
};
export type ExamRequest = {
  source: "wordbook" | "favorites" | "mistakes";
  wordbookId: number | null;
  enToZhCount: number;
  zhToEnCount: number;
};
export type ExamQuestion = {
  entry: WordEntry;
  direction: "zh-to-en" | "en-to-zh";
};

export type ScheduleRequest = {
  source: "wordbook" | "favorites" | "mistakes";
  wordbookId: number | null;
  limit: number;
  mode: "zh-to-en" | "en-to-zh" | "mixed";
};

export type ReviewOutcome = {
  reviewId: number;
  errorCount: number;
  hintCount: number;
  skipped: boolean;
};

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
  reviewTarget(): Promise<number>;
  setReviewTarget(target: number): Promise<void>;
  schedulePractice(request: ScheduleRequest): Promise<ScheduledQuestion[]>;
  sampleExam(request: ExamRequest): Promise<ExamQuestion[]>;
  completeReview(outcome: ReviewOutcome): Promise<void>;
  listWordbooks(): Promise<WordbookSummary[]>;
  importWordbook(request: ImportWordbookRequest): Promise<ImportWordbookResult>;
  sampleWordbook(wordbookId: number, limit: number): Promise<WordEntry[]>;
  deleteWordbook(wordbookId: number): Promise<boolean>;
  deleteWordbookEntry(wordbookId: number, entryId: number): Promise<boolean>;
  addFavorite(english: string, chinese: string): Promise<WordEntry>;
  removeFavorite(english: string, chinese: string): Promise<boolean>;
  isFavorite(english: string, chinese: string): Promise<boolean>;
  listFavorites(): Promise<WordEntry[]>;
  sampleFavorites(limit: number): Promise<WordEntry[]>;
  recordMistake(english: string, chinese: string): Promise<MistakeEntry>;
  recordMistakeOnce(
    english: string,
    chinese: string,
    submissionId: string,
  ): Promise<MistakeEntry>;
  listMistakes(): Promise<MistakeEntry[]>;
  sampleMistakes(limit: number): Promise<WordEntry[]>;
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
  reviewTarget() {
    return invoke<number>("review_target").catch((error) => {
      throw commandError(error);
    });
  },
  setReviewTarget(target) {
    return invoke<void>("set_review_target", { target }).catch((error) => {
      throw commandError(error);
    });
  },
  sampleExam(request) {
    return invoke<ExamQuestion[]>("sample_exam", request).catch((error) => {
      throw commandError(error);
    });
  },
  schedulePractice(request) {
    return invoke<ScheduledQuestion[]>("schedule_practice", request).catch(
      (error) => {
        throw commandError(error);
      },
    );
  },
  completeReview(outcome) {
    return invoke<void>("complete_review", outcome).catch((error) => {
      throw commandError(error);
    });
  },
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
  deleteWordbook(wordbookId) {
    return invoke<boolean>("delete_wordbook", { wordbookId }).catch((error) => {
      throw commandError(error);
    });
  },
  deleteWordbookEntry(wordbookId, entryId) {
    return invoke<boolean>("delete_wordbook_entry", {
      wordbookId,
      entryId,
    }).catch((error) => {
      throw commandError(error);
    });
  },
  addFavorite(english, chinese) {
    return invoke<WordEntry>("add_favorite", { english, chinese }).catch(
      (error) => {
        throw commandError(error);
      },
    );
  },
  removeFavorite(english, chinese) {
    return invoke<boolean>("remove_favorite", { english, chinese }).catch(
      (error) => {
        throw commandError(error);
      },
    );
  },
  isFavorite(english, chinese) {
    return invoke<boolean>("is_favorite", { english, chinese }).catch(
      (error) => {
        throw commandError(error);
      },
    );
  },
  listFavorites() {
    return invoke<WordEntry[]>("list_favorites").catch((error) => {
      throw commandError(error);
    });
  },
  sampleFavorites(limit) {
    return invoke<WordEntry[]>("sample_favorites", { limit }).catch((error) => {
      throw commandError(error);
    });
  },
  recordMistake(english, chinese) {
    return invoke<MistakeEntry>("record_mistake", { english, chinese }).catch(
      (error) => {
        throw commandError(error);
      },
    );
  },
  recordMistakeOnce(english, chinese, submissionId) {
    return invoke<MistakeEntry>("record_mistake_once", {
      english,
      chinese,
      submissionId,
    }).catch((error) => {
      throw commandError(error);
    });
  },
  listMistakes() {
    return invoke<MistakeEntry[]>("list_mistakes").catch((error) => {
      throw commandError(error);
    });
  },
  sampleMistakes(limit) {
    return invoke<WordEntry[]>("sample_mistakes", { limit }).catch((error) => {
      throw commandError(error);
    });
  },
};

export function defaultWordbookName(path: string): string {
  const parts = path.split(/[\\/]/);
  const fileName = parts[parts.length - 1] ?? path;
  return fileName.replace(/\.(xlsx|xls)$/i, "");
}

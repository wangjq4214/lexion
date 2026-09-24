import { render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { describe, expect, it, vi } from "vitest";
import { WordbookError, type WordbookService } from "../../data/wordbooks";
import { WordbookImportFlow } from "./WordbookImportFlow";

function service(overrides?: Partial<WordbookService>): WordbookService {
  return {
    reviewTarget: vi.fn(async () => 0.9),
    setReviewTarget: vi.fn(async () => {}),
    schedulePractice: vi.fn(async () => []),
    completeReview: vi.fn(async () => {}),
    pickWorkbookFile: vi.fn(async () => "C:\\imports\\lesson.xls"),
    listWordbooks: vi.fn(async () => []),
    importWordbook: vi.fn(async (request) => ({
      wordbook: { id: 1, name: request.name, entryCount: 3 },
    })),
    deleteWordbook: vi.fn(async () => true),
    deleteWordbookEntry: vi.fn(async () => true),
    sampleWordbook: vi.fn(async () => []),
    addFavorite: vi.fn(async (english, chinese) => ({
      id: 1,
      english,
      chinese,
    })),
    removeFavorite: vi.fn(async () => true),
    isFavorite: vi.fn(async () => false),
    listFavorites: vi.fn(async () => []),
    sampleFavorites: vi.fn(async () => []),
    recordMistake: vi.fn(async (english, chinese) => ({
      id: 1,
      english,
      chinese,
      errorCount: 1,
    })),
    recordMistakeOnce: vi.fn(async (english, chinese) => ({
      id: 1,
      english,
      chinese,
      errorCount: 1,
    })),
    listMistakes: vi.fn(async () => []),
    sampleMistakes: vi.fn(async () => []),
    ...overrides,
  };
}

describe("WordbookImportFlow", () => {
  it("does nothing when native file selection is cancelled", async () => {
    const user = userEvent.setup();
    const wordbooks = service({ pickWorkbookFile: vi.fn(async () => null) });
    render(<WordbookImportFlow service={wordbooks} onImported={vi.fn()} />);

    await user.click(screen.getByRole("button", { name: "导入单词本" }));

    expect(screen.queryByRole("dialog")).not.toBeInTheDocument();
    expect(wordbooks.importWordbook).not.toHaveBeenCalled();
  });

  it("uses an editable file-name default and reports validation errors inline", async () => {
    const user = userEvent.setup();
    const wordbooks = service({
      importWordbook: vi.fn(async () => {
        throw new WordbookError("validation", "缺少 chinese 列");
      }),
    });
    render(<WordbookImportFlow service={wordbooks} onImported={vi.fn()} />);

    await user.click(screen.getByRole("button", { name: "导入单词本" }));
    const name = await screen.findByLabelText("单词本名称");
    expect(name).toHaveValue("lesson");
    await user.clear(name);
    await user.type(name, "第 一 册");
    await user.click(screen.getByRole("button", { name: "导入" }));

    expect(await screen.findByText("缺少 chinese 列")).toBeInTheDocument();
    expect(wordbooks.importWordbook).toHaveBeenCalledWith({
      path: "C:\\imports\\lesson.xls",
      name: "第 一 册",
      replaceExisting: false,
    });
  });

  it("moves a same-name conflict into a separate confirmation dialog", async () => {
    const user = userEvent.setup();
    let attempts = 0;
    const wordbooks = service({
      importWordbook: vi.fn(async (request) => {
        attempts += 1;
        if (!request.replaceExisting) {
          throw new WordbookError("conflict", "");
        }
        return { wordbook: { id: 2, name: request.name, entryCount: 4 } };
      }),
    });
    const onImported = vi.fn();
    render(<WordbookImportFlow service={wordbooks} onImported={onImported} />);

    await user.click(screen.getByRole("button", { name: "导入单词本" }));
    await user.click(await screen.findByRole("button", { name: "导入" }));

    expect(await screen.findByRole("alertdialog")).toBeInTheDocument();
    expect(screen.queryByRole("dialog")).not.toBeInTheDocument();
    await user.click(screen.getByRole("button", { name: "替换单词本" }));

    expect(attempts).toBe(2);
    expect(onImported).toHaveBeenCalledWith({
      wordbook: { id: 2, name: "lesson", entryCount: 4 },
    });
  });
});

import { act, render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { describe, expect, it, vi } from "vitest";
import App from "./App";
import type {
  ImportWordbookRequest,
  WordbookService,
  WordbookSummary,
} from "./data/wordbooks";
import type { WordEntry } from "./domain/word";

const stableRandom = () => 0.9;

function createService(options?: {
  wordbooks?: WordbookSummary[];
  samples?: Record<number, WordEntry[]>;
  selectedPath?: string | null;
}) {
  let wordbooks = options?.wordbooks ?? [];
  const samples = options?.samples ?? {};
  const service: WordbookService = {
    pickWorkbookFile: vi.fn(async () => options?.selectedPath ?? null),
    listWordbooks: vi.fn(async () => wordbooks),
    importWordbook: vi.fn(async (request: ImportWordbookRequest) => {
      const imported = { id: 99, name: request.name, entryCount: 2 };
      wordbooks = [...wordbooks, imported];
      samples[imported.id] = [
        { id: 991, english: "apple", chinese: "苹果" },
        { id: 992, english: "book", chinese: "书" },
      ];
      return { wordbook: imported };
    }),
    sampleWordbook: vi.fn(
      async (wordbookId: number) => samples[wordbookId] ?? [],
    ),
  };
  return service;
}

describe("App wordbook practice", () => {
  it("shows only the import action when empty and refreshes after import", async () => {
    const user = userEvent.setup();
    const service = createService({ selectedPath: "C:\\books\\starter.xlsx" });
    render(<App random={stableRandom} wordbookService={service} />);

    expect(
      await screen.findByRole("button", { name: "导入单词本" }),
    ).toBeInTheDocument();
    expect(screen.queryByRole("heading")).not.toBeInTheDocument();

    await user.click(screen.getByRole("button", { name: "导入单词本" }));
    expect(await screen.findByLabelText("单词本名称")).toHaveValue("starter");
    await user.click(screen.getByRole("button", { name: "导入" }));

    expect(
      await screen.findByRole("heading", { name: "练习设置" }),
    ).toBeInTheDocument();
    expect(screen.getByLabelText("当前单词本")).toHaveTextContent("starter");
    expect(
      screen.getByRole("button", { name: "导入单词本" }),
    ).toBeInTheDocument();
  });

  it("loads a small sample and uses its dynamic progress and summary total", async () => {
    const user = userEvent.setup();
    const entries = [
      { id: 1, english: "apple", chinese: "苹果" },
      { id: 2, english: "book", chinese: "书" },
    ];
    const service = createService({
      wordbooks: [{ id: 1, name: "基础", entryCount: 2 }],
      samples: { 1: entries },
    });
    render(<App random={stableRandom} wordbookService={service} />);

    await user.click(await screen.findByRole("button", { name: "开始练习" }));
    expect(service.sampleWordbook).toHaveBeenCalledWith(1, 10);
    expect(await screen.findByText("0 / 2")).toBeInTheDocument();

    for (const entry of entries) {
      const input = screen.getByLabelText("英文答案");
      await user.type(input, entry.english.toUpperCase());
      await user.type(input, "{Enter}");
    }

    expect(
      screen.getByRole("heading", { name: "本轮练习完成" }),
    ).toBeInTheDocument();
    expect(screen.getByText("答对题数：2 / 2")).toBeInTheDocument();
  });

  it("locks wordbook and mode controls while a sample is loading", async () => {
    const user = userEvent.setup();
    let resolveSample: (entries: WordEntry[]) => void = () => undefined;
    const sample = new Promise<WordEntry[]>((resolve) => {
      resolveSample = resolve;
    });
    const service = createService({
      wordbooks: [{ id: 1, name: "基础", entryCount: 1 }],
    });
    service.sampleWordbook = vi.fn(async () => sample);
    render(<App wordbookService={service} />);

    await user.click(await screen.findByRole("button", { name: "开始练习" }));

    expect(screen.getByLabelText("当前单词本")).toBeDisabled();
    expect(screen.getByRole("button", { name: "看中文拼英文" })).toBeDisabled();
    expect(screen.getByRole("button", { name: "导入单词本" })).toBeDisabled();
    await act(async () => {
      resolveSample([{ id: 1, english: "apple", chinese: "苹果" }]);
      await sample;
    });
    expect(
      await screen.findByRole("heading", { name: "苹果" }),
    ).toBeInTheDocument();
  });

  it("keeps practice errors visible without leaving setup", async () => {
    const user = userEvent.setup();
    const service = createService({
      wordbooks: [{ id: 1, name: "基础", entryCount: 1 }],
      samples: { 1: [] },
    });
    render(<App wordbookService={service} />);

    await user.click(await screen.findByRole("button", { name: "开始练习" }));
    expect(await screen.findByRole("alert")).toHaveTextContent(
      "没有可练习的词条",
    );
    await waitFor(() =>
      expect(screen.getByRole("button", { name: "开始练习" })).toBeEnabled(),
    );
  });
});

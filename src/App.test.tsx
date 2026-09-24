import {
  act,
  fireEvent,
  render,
  screen,
  waitFor,
} from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { afterEach, describe, expect, it, vi } from "vitest";
import App from "./App";
import type {
  ImportWordbookRequest,
  MistakeEntry,
  WordbookService,
  WordbookSummary,
} from "./data/wordbooks";
import type { WordEntry } from "./domain/word";

const stableRandom = () => 0.9;

function createService(options?: {
  wordbooks?: WordbookSummary[];
  samples?: Record<number, WordEntry[]>;
  selectedPath?: string | null;
  favorites?: WordEntry[];
  mistakes?: MistakeEntry[];
}) {
  let wordbooks = options?.wordbooks ?? [];
  const samples = options?.samples ?? {};
  let favorites = [...(options?.favorites ?? [])];
  let mistakes = [...(options?.mistakes ?? [])];
  const savedSubmissions = new Map<string, MistakeEntry>();
  let nextReviewId = 1;
  const service: WordbookService = {
    pickWorkbookFile: vi.fn(async () => options?.selectedPath ?? null),
    reviewTarget: vi.fn(async () => 0.9),
    setReviewTarget: vi.fn(async () => {}),
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
    deleteWordbookEntry: vi.fn(async (wordbookId: number, entryId: number) => {
      const entries = samples[wordbookId] ?? [];
      const found = entries.some((entry) => entry.id === entryId);
      samples[wordbookId] = entries.filter((entry) => entry.id !== entryId);
      return found;
    }),
    sampleWordbook: vi.fn(
      async (wordbookId: number) => samples[wordbookId] ?? [],
    ),
    addFavorite: vi.fn(async (english: string, chinese: string) => {
      const existing = favorites.find(
        (entry) => entry.english === english && entry.chinese === chinese,
      );
      if (existing) return existing;
      const added = { id: 101 + favorites.length, english, chinese };
      favorites = [...favorites, added];
      return added;
    }),
    removeFavorite: vi.fn(async (english: string, chinese: string) => {
      const existed = favorites.some(
        (entry) => entry.english === english && entry.chinese === chinese,
      );
      favorites = favorites.filter(
        (entry) => entry.english !== english || entry.chinese !== chinese,
      );
      return existed;
    }),
    isFavorite: vi.fn(async (english: string, chinese: string) =>
      favorites.some(
        (entry) => entry.english === english && entry.chinese === chinese,
      ),
    ),
    listFavorites: vi.fn(async () => [...favorites]),
    sampleFavorites: vi.fn(async (limit: number) => favorites.slice(0, limit)),
    recordMistake: vi.fn(async (english: string, chinese: string) => {
      const existing = mistakes.find(
        (entry) =>
          entry.english.toLowerCase() === english.toLowerCase() &&
          entry.chinese === chinese,
      );
      if (existing) {
        const updated = { ...existing, errorCount: existing.errorCount + 1 };
        mistakes = mistakes.map((entry) =>
          entry.id === existing.id ? updated : entry,
        );
        return updated;
      }
      const added = {
        id: 201 + mistakes.length,
        english,
        chinese,
        errorCount: 1,
      };
      mistakes = [...mistakes, added];
      return added;
    }),
    recordMistakeOnce: vi.fn(
      async (english: string, chinese: string, submissionId: string) => {
        const saved = savedSubmissions.get(submissionId);
        if (saved) return saved;
        const result = await service.recordMistake(english, chinese);
        savedSubmissions.set(submissionId, result);
        return result;
      },
    ),
    listMistakes: vi.fn(async () => [...mistakes]),
    sampleMistakes: vi.fn(async (limit: number) => mistakes.slice(0, limit)),
    schedulePractice: vi.fn(async ({ source, wordbookId, limit, mode }) => {
      const entries =
        source === "favorites"
          ? await service.sampleFavorites(limit)
          : source === "mistakes"
            ? await service.sampleMistakes(limit)
            : await service.sampleWordbook(wordbookId ?? 0, limit);
      return entries.map((entry) => ({
        reviewId: nextReviewId++,
        entry,
        direction: mode === "mixed" ? ("en-to-zh" as const) : mode,
      }));
    }),
    completeReview: vi.fn(async () => {}),
  };
  return service;
}

describe("App wordbook practice", () => {
  afterEach(() => {
    vi.useRealTimers();
  });

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
    expect(
      screen.getByRole("heading", { name: "选择练习内容" }),
    ).toBeInTheDocument();
    expect(
      screen.getByRole("heading", { name: "设置练习方式" }),
    ).toBeInTheDocument();
    expect(
      screen.getByRole("heading", { name: "浏览词库" }),
    ).toBeInTheDocument();
    expect(screen.getByLabelText("练习模式")).toHaveTextContent("看中文拼英文");
    expect(
      screen.queryByRole("button", { name: "看中文拼英文" }),
    ).not.toBeInTheDocument();
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

  it.each([5, 20, 50])("requests the %i-word preset", async (count) => {
    const user = userEvent.setup();
    const service = createService({
      wordbooks: [{ id: 1, name: "基础", entryCount: 100 }],
      samples: { 1: [{ id: 1, english: "apple", chinese: "苹果" }] },
    });
    render(<App wordbookService={service} />);
    await user.click(await screen.findByLabelText("本轮单词数"));
    await user.click(
      await screen.findByRole("option", { name: `${count} 个单词` }),
    );
    await user.click(screen.getByRole("button", { name: "开始练习" }));
    expect(service.sampleWordbook).toHaveBeenCalledWith(1, count);
    expect(await screen.findByText("0 / 1")).toBeInTheDocument();
  });

  it("tracks actual progress above ten questions even when fewer than requested", async () => {
    const user = userEvent.setup();
    const entries = Array.from({ length: 12 }, (_, index) => ({
      id: index + 1,
      english: `word-${index + 1}`,
      chinese: `释义-${index + 1}`,
    }));
    const service = createService({
      wordbooks: [{ id: 1, name: "基础", entryCount: 12 }],
      samples: { 1: entries },
    });
    render(<App random={stableRandom} wordbookService={service} />);
    await user.click(await screen.findByLabelText("本轮单词数"));
    await user.click(await screen.findByRole("option", { name: "20 个单词" }));
    await user.click(screen.getByRole("button", { name: "开始练习" }));
    expect(service.sampleWordbook).toHaveBeenCalledWith(1, 20);
    expect(await screen.findByText("0 / 12")).toBeInTheDocument();
    for (let index = 0; index < entries.length; index += 1) {
      const prompt = screen.getByRole("heading", { level: 1 }).textContent;
      const answer = entries.find((entry) => entry.chinese === prompt)?.english;
      expect(answer).toBeDefined();
      await user.type(screen.getByLabelText("英文答案"), `${answer}{Enter}`);
    }
    expect(screen.getByText("答对题数：12 / 12")).toBeInTheDocument();
  });

  it("validates custom counts and reuses the selection on another round", async () => {
    const user = userEvent.setup();
    const service = createService({
      wordbooks: [{ id: 1, name: "基础", entryCount: 1 }],
      samples: { 1: [{ id: 1, english: "apple", chinese: "苹果" }] },
    });
    render(<App wordbookService={service} />);
    await user.click(await screen.findByLabelText("本轮单词数"));
    await user.click(await screen.findByRole("option", { name: "自定义" }));
    const input = screen.getByLabelText("自定义单词数");
    const start = screen.getByRole("button", { name: "开始练习" });
    expect(start).toBeDisabled();
    for (const invalid of ["0", "256", "2.5", "abc", "-1"]) {
      await user.clear(input);
      await user.type(input, invalid);
      expect(start).toBeDisabled();
      expect(input).toHaveAttribute("aria-invalid", "true");
    }
    expect(service.sampleWordbook).not.toHaveBeenCalled();
    await user.clear(input);
    await user.type(input, "1");
    expect(start).toBeEnabled();
    await user.clear(input);
    await user.type(input, "255");
    expect(start).toBeEnabled();
    await user.click(start);
    expect(service.sampleWordbook).toHaveBeenCalledWith(1, 255);
    expect(await screen.findByText("0 / 1")).toBeInTheDocument();
    await user.type(screen.getByLabelText("英文答案"), "apple{Enter}");
    expect(screen.getByText("答对题数：1 / 1")).toBeInTheDocument();
    await user.click(screen.getByRole("button", { name: "再练一轮" }));
    expect(service.sampleWordbook).toHaveBeenLastCalledWith(1, 255);
  });
  it("uses the selected count with a different wordbook", async () => {
    const user = userEvent.setup();
    const service = createService({
      wordbooks: [
        { id: 1, name: "基础", entryCount: 1 },
        { id: 2, name: "进阶", entryCount: 1 },
      ],
      samples: { 2: [{ id: 2, english: "book", chinese: "书" }] },
    });
    render(<App wordbookService={service} />);
    await user.click(await screen.findByLabelText("当前单词本"));
    await user.click(await screen.findByRole("option", { name: /进阶/ }));
    await user.click(screen.getByLabelText("本轮单词数"));
    await user.click(await screen.findByRole("option", { name: "5 个单词" }));
    await user.click(screen.getByRole("button", { name: "开始练习" }));
    expect(service.sampleWordbook).toHaveBeenCalledWith(2, 5);
    expect(
      await screen.findByRole("heading", { name: "书" }),
    ).toBeInTheDocument();
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

    await user.click(await screen.findByLabelText("本轮单词数"));
    await user.click(await screen.findByRole("option", { name: "自定义" }));
    await user.type(screen.getByLabelText("自定义单词数"), "20");
    await user.click(await screen.findByRole("button", { name: "开始练习" }));

    expect(screen.getByLabelText("当前单词本")).toBeDisabled();
    expect(screen.getByLabelText("本轮单词数")).toBeDisabled();
    expect(screen.getByLabelText("自定义单词数")).toBeDisabled();
    expect(service.sampleWordbook).toHaveBeenCalledWith(1, 20);
    expect(screen.getByLabelText("练习模式")).toBeDisabled();
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
    expect(
      await screen.findByText("这个单词本没有可练习的词条。"),
    ).toBeInTheDocument();
    await waitFor(() =>
      expect(screen.getByRole("button", { name: "开始练习" })).toBeEnabled(),
    );
  });

  it("tracks elapsed time while viewing a wrong answer and across questions", async () => {
    let now = 0;
    const entries = [
      { id: 1, english: "apple", chinese: "苹果" },
      { id: 2, english: "book", chinese: "书" },
    ];
    const service = createService({
      wordbooks: [{ id: 1, name: "基础", entryCount: 2 }],
      samples: { 1: entries },
    });

    render(
      <App now={() => now} random={stableRandom} wordbookService={service} />,
    );

    const startButton = await screen.findByRole("button", {
      name: "开始练习",
    });
    vi.useFakeTimers();
    fireEvent.click(startButton);
    await act(async () => undefined);
    expect(screen.getByText("本轮用时：0:00")).toBeInTheDocument();

    now = 59_000;
    act(() => vi.advanceTimersByTime(1_000));
    expect(screen.getByText("本轮用时：0:59")).toBeInTheDocument();

    const firstAnswer = screen.getByLabelText("英文答案");
    fireEvent.change(firstAnswer, { target: { value: "wrong" } });
    fireEvent.keyDown(firstAnswer, { key: "Enter", code: "Enter" });
    await act(async () => undefined);
    expect(screen.getByText(/本题答错/)).toBeInTheDocument();
    now = 60_000;
    act(() => vi.advanceTimersByTime(1_000));
    expect(screen.getByText("本轮用时：1:00")).toBeInTheDocument();

    fireEvent.click(screen.getByRole("button", { name: "下一题" }));
    await act(async () => undefined);
    expect(screen.getByText("本轮用时：1:00")).toBeInTheDocument();

    now = 65_900;
    const secondAnswer = screen.getByLabelText("英文答案");
    fireEvent.change(secondAnswer, { target: { value: "book" } });
    fireEvent.keyDown(secondAnswer, { key: "Enter", code: "Enter" });
    await act(async () => undefined);

    expect(
      screen.getByRole("heading", { name: "本轮练习完成" }),
    ).toBeInTheDocument();
    expect(screen.getByText("总用时：1:05")).toBeInTheDocument();

    now = 120_000;
    act(() => vi.advanceTimersByTime(10_000));
    expect(screen.getByText("总用时：1:05")).toBeInTheDocument();

    fireEvent.click(screen.getByRole("button", { name: "再练一轮" }));
    await act(async () => undefined);
    expect(screen.getByText("本轮用时：0:00")).toBeInTheDocument();
  });

  it("advances three hint levels and treats the third as a skip", async () => {
    const user = userEvent.setup();
    const service = createService({
      wordbooks: [{ id: 1, name: "基础", entryCount: 1 }],
      samples: { 1: [{ id: 1, english: "abcdef", chinese: "示例" }] },
    });
    render(<App random={stableRandom} wordbookService={service} />);
    await user.click(await screen.findByRole("button", { name: "开始练习" }));
    await user.click(screen.getByRole("button", { name: "显示第 1 级提示" }));
    expect(screen.getByText("拼写提示（第 1 级）")).toBeInTheDocument();
    const first = screen.getByText(/_/).textContent ?? "";
    await user.click(screen.getByRole("button", { name: "显示第 2 级提示" }));
    expect(screen.getByText("拼写提示（第 2 级）")).toBeInTheDocument();
    const second = screen.getByText(/_/).textContent ?? "";
    expect(first.split(" ").filter((letter) => letter !== "_")).toHaveLength(2);
    expect(second.split(" ").filter((letter) => letter !== "_")).toHaveLength(
      4,
    );
    await user.click(
      screen.getByRole("button", { name: "显示第 3 级提示并跳过" }),
    );
    expect(screen.getByText("英文：abcdef")).toBeInTheDocument();
    expect(screen.getByText("中文释义：示例")).toBeInTheDocument();
    expect(
      screen.queryByRole("button", { name: "提交答案" }),
    ).not.toBeInTheDocument();
    await user.click(screen.getByRole("button", { name: "查看练习结果" }));
    expect(screen.getByText("答对题数：0 / 1")).toBeInTheDocument();
    expect(screen.getByText("提示次数：3")).toBeInTheDocument();
    expect(screen.getByText("跳过次数：1")).toBeInTheDocument();
  });

  it("retains used hints on direct skip and resets hint levels in a new round", async () => {
    const user = userEvent.setup();
    const service = createService({
      wordbooks: [{ id: 1, name: "基础", entryCount: 1 }],
      samples: { 1: [{ id: 1, english: "apple", chinese: "苹果" }] },
    });
    render(<App random={stableRandom} wordbookService={service} />);
    await user.click(await screen.findByRole("button", { name: "开始练习" }));
    await user.click(screen.getByRole("button", { name: "显示第 1 级提示" }));
    await user.click(screen.getByRole("button", { name: "显示第 2 级提示" }));
    await user.click(screen.getByRole("button", { name: "跳过" }));
    await user.click(screen.getByRole("button", { name: "查看练习结果" }));
    expect(screen.getByText("提示次数：2")).toBeInTheDocument();
    expect(screen.getByText("跳过次数：1")).toBeInTheDocument();
    await user.click(screen.getByRole("button", { name: "再练一轮" }));
    expect(
      await screen.findByRole("button", { name: "显示第 1 级提示" }),
    ).toBeInTheDocument();
    await user.type(screen.getByLabelText("英文答案"), "apple{Enter}");
    expect(screen.getByText("提示次数：0")).toBeInTheDocument();
    expect(screen.getByText("跳过次数：0")).toBeInTheDocument();
  });

  it("allows a correct answer after partial hints and resets on the next question", async () => {
    const user = userEvent.setup();
    const service = createService({
      wordbooks: [{ id: 1, name: "基础", entryCount: 2 }],
      samples: {
        1: [
          { id: 1, english: "apple", chinese: "苹果" },
          { id: 2, english: "book", chinese: "书" },
        ],
      },
    });
    render(<App random={stableRandom} wordbookService={service} />);
    await user.click(await screen.findByRole("button", { name: "开始练习" }));
    await user.click(screen.getByRole("button", { name: "显示第 1 级提示" }));
    await user.click(screen.getByRole("button", { name: "显示第 2 级提示" }));
    await user.type(screen.getByLabelText("英文答案"), "apple{Enter}");
    expect(
      screen.getByRole("button", { name: "显示第 1 级提示" }),
    ).toBeInTheDocument();
    await user.type(screen.getByLabelText("英文答案"), "book{Enter}");
    expect(screen.getByText("答对题数：2 / 2")).toBeInTheDocument();
    expect(screen.getByText("提示次数：2")).toBeInTheDocument();
    expect(screen.getByText("跳过次数：0")).toBeInTheDocument();
  });

  it("reveals a skipped word and counts only answered questions", async () => {
    const user = userEvent.setup();
    const service = createService({
      wordbooks: [{ id: 1, name: "基础", entryCount: 2 }],
      samples: {
        1: [
          { id: 1, english: "apple", chinese: "苹果" },
          { id: 2, english: "book", chinese: "书" },
        ],
      },
    });
    render(<App random={stableRandom} wordbookService={service} />);
    await user.click(await screen.findByRole("button", { name: "开始练习" }));
    await user.click(screen.getByRole("button", { name: "跳过" }));
    expect(screen.getByText("英文：apple")).toBeInTheDocument();
    expect(screen.getByText("中文释义：苹果")).toBeInTheDocument();
    expect(screen.getByText("0 / 2")).toBeInTheDocument();
    expect(
      screen.queryByRole("button", { name: "跳过" }),
    ).not.toBeInTheDocument();
    expect(
      screen.queryByRole("button", { name: "提交答案" }),
    ).not.toBeInTheDocument();
    await user.click(screen.getByRole("button", { name: "下一题" }));
    await user.type(screen.getByLabelText("英文答案"), "book{Enter}");
    expect(screen.getByText("答对题数：1 / 2")).toBeInTheDocument();
    expect(screen.getByText("跳过次数：1")).toBeInTheDocument();
    expect(screen.queryByText("英文：apple")).not.toBeInTheDocument();
    await user.click(screen.getByRole("button", { name: "再练一轮" }));
    expect(
      await screen.findByRole("button", { name: "跳过" }),
    ).toBeInTheDocument();
  });

  it("reveals both sides in mixed mode and resets the skipped count for a new round", async () => {
    const user = userEvent.setup();
    const service = createService({
      wordbooks: [{ id: 1, name: "基础", entryCount: 1 }],
      samples: { 1: [{ id: 1, english: "apple", chinese: "苹果" }] },
    });
    render(<App random={stableRandom} wordbookService={service} />);
    await user.click(await screen.findByLabelText("练习模式"));
    await user.click(
      await screen.findByRole("option", { name: "双向混合随机" }),
    );
    await user.click(screen.getByRole("button", { name: "开始练习" }));
    await user.click(screen.getByRole("button", { name: "跳过" }));
    expect(screen.getByText("英文：apple")).toBeInTheDocument();
    expect(screen.getByText("中文释义：苹果")).toBeInTheDocument();
    await user.click(screen.getByRole("button", { name: "查看练习结果" }));
    expect(screen.getByText("跳过次数：1")).toBeInTheDocument();
    await user.click(screen.getByRole("button", { name: "再练一轮" }));
    await user.type(await screen.findByLabelText("中文答案"), "苹果{Enter}");
    expect(screen.getByText("答对题数：1 / 1")).toBeInTheDocument();
    expect(screen.getByText("跳过次数：0")).toBeInTheDocument();
  });

  it("shows Chinese answer differences on the last question without counting a skip", async () => {
    const user = userEvent.setup();
    const service = createService({
      wordbooks: [{ id: 1, name: "基础", entryCount: 1 }],
      samples: { 1: [{ id: 1, english: "apple", chinese: "苹果" }] },
    });
    render(<App random={stableRandom} wordbookService={service} />);
    await user.click(await screen.findByLabelText("练习模式"));
    await user.click(
      await screen.findByRole("option", { name: "看英文拼中文" }),
    );
    await user.click(screen.getByRole("button", { name: "开始练习" }));
    await user.type(screen.getByLabelText("中文答案"), "错{Enter}");
    expect(screen.getByText(/本题答错/)).toBeInTheDocument();
    expect(
      screen.getByText(
        (_, element) =>
          element?.tagName === "SPAN" && element.textContent === "你的答案：错",
      ),
    ).toBeInTheDocument();
    expect(
      screen.getByText(
        (_, element) =>
          element?.tagName === "SPAN" &&
          element.textContent === "正确答案：苹果",
      ),
    ).toBeInTheDocument();
    expect(
      screen.getByText(/差异说明：你的答案需核对 第 1 位「错」/),
    ).toBeInTheDocument();
    expect(screen.queryByLabelText("中文答案")).not.toBeInTheDocument();
    expect(
      screen.queryByRole("heading", { name: "本轮练习完成" }),
    ).not.toBeInTheDocument();
    await user.click(screen.getByRole("button", { name: "查看练习结果" }));
    expect(screen.getByText("答对题数：0 / 1")).toBeInTheDocument();
    expect(screen.getByText("错误次数：1")).toBeInTheDocument();
    expect(screen.getByText("跳过次数：0")).toBeInTheDocument();
    expect(service.completeReview).toHaveBeenCalledWith({
      reviewId: 1,
      errorCount: 1,
      hintCount: 0,
      skipped: false,
    });
  });

  it("includes hint use and answer reveal time when all questions are skipped", async () => {
    let now = 0;
    const service = createService({
      wordbooks: [{ id: 1, name: "基础", entryCount: 1 }],
      samples: { 1: [{ id: 1, english: "apple", chinese: "苹果" }] },
    });
    render(
      <App now={() => now} random={stableRandom} wordbookService={service} />,
    );
    const start = await screen.findByRole("button", { name: "开始练习" });
    vi.useFakeTimers();
    fireEvent.click(start);
    await act(async () => undefined);
    fireEvent.click(screen.getByRole("button", { name: "显示第 1 级提示" }));
    now = 5_000;
    fireEvent.click(screen.getByRole("button", { name: "跳过" }));
    now = 8_000;
    act(() => vi.advanceTimersByTime(1_000));
    expect(screen.getByText("本轮用时：0:08")).toBeInTheDocument();
    fireEvent.click(screen.getByRole("button", { name: "查看练习结果" }));
    await act(async () => undefined);
    expect(screen.getByText("总用时：0:08")).toBeInTheDocument();
    expect(screen.getByText("提示次数：1")).toBeInTheDocument();
    expect(screen.getByText("跳过次数：1")).toBeInTheDocument();
    now = 20_000;
    act(() => vi.advanceTimersByTime(1_000));
    expect(screen.getByText("总用时：0:08")).toBeInTheDocument();
  });

  it("finalizes the current elapsed time when the submit button is used", async () => {
    let now = 0;
    const service = createService({
      wordbooks: [{ id: 1, name: "基础", entryCount: 1 }],
      samples: {
        1: [{ id: 1, english: "apple", chinese: "苹果" }],
      },
    });

    render(
      <App now={() => now} random={stableRandom} wordbookService={service} />,
    );

    const startButton = await screen.findByRole("button", {
      name: "开始练习",
    });
    vi.useFakeTimers();
    fireEvent.click(startButton);
    await act(async () => undefined);
    fireEvent.change(screen.getByLabelText("英文答案"), {
      target: { value: "apple" },
    });
    now = 1_999;
    fireEvent.click(screen.getByRole("button", { name: "提交答案" }));
    await act(async () => undefined);

    expect(screen.getByText("总用时：0:01")).toBeInTheDocument();
  });

  it("adds and removes the current pair even after skip reveals its answer", async () => {
    const user = userEvent.setup();
    const service = createService({
      wordbooks: [{ id: 1, name: "基础", entryCount: 1 }],
      samples: { 1: [{ id: 1, english: "apple", chinese: "苹果" }] },
    });
    render(<App wordbookService={service} />);
    await user.click(await screen.findByRole("button", { name: "开始练习" }));
    const prompt = await screen.findByRole("heading", { name: "苹果" });
    const star = await screen.findByRole("button", { name: "收藏这个单词" });
    expect(prompt.parentElement).toContainElement(star);
    expect(star).toHaveAttribute("aria-pressed", "false");
    expect(star).toHaveTextContent("☆");
    await user.click(
      await screen.findByRole("button", { name: "收藏这个单词" }),
    );
    expect(service.addFavorite).toHaveBeenCalledWith("apple", "苹果");
    expect((await screen.findAllByText("已收藏")).length).toBeGreaterThan(0);
    expect(screen.getByRole("button", { name: "取消收藏" })).toHaveAttribute(
      "aria-pressed",
      "true",
    );
    expect(screen.getByRole("button", { name: "取消收藏" })).toHaveTextContent(
      "★",
    );
    await user.click(screen.getByRole("button", { name: "跳过" }));
    expect(screen.getByText("英文：apple")).toBeInTheDocument();
    await user.click(await screen.findByRole("button", { name: "取消收藏" }));
    expect(service.removeFavorite).toHaveBeenCalledWith("apple", "苹果");
    expect((await screen.findAllByText("已取消")).length).toBeGreaterThan(0);
    expect(
      await screen.findByRole("button", { name: "收藏这个单词" }),
    ).toBeEnabled();
  });

  it("lists saved pairs, preserves a failed removal, and removes a pair on retry", async () => {
    const user = userEvent.setup();
    const service = createService({
      wordbooks: [{ id: 1, name: "基础", entryCount: 1 }],
      favorites: [{ id: 7, english: "apple", chinese: "苹果" }],
    });
    vi.mocked(service.removeFavorite).mockRejectedValueOnce(
      new Error("删除失败"),
    );
    render(<App wordbookService={service} />);
    await user.click(await screen.findByRole("button", { name: "查看收藏夹" }));
    expect(await screen.findByText("苹果")).toBeInTheDocument();
    await user.click(screen.getByRole("button", { name: "移除 apple：苹果" }));
    expect(await screen.findByText("删除失败")).toHaveAttribute(
      "role",
      "alert",
    );
    expect(screen.getByText("苹果")).toBeInTheDocument();
    await user.click(screen.getByRole("button", { name: "移除 apple：苹果" }));
    expect(service.removeFavorite).toHaveBeenLastCalledWith("apple", "苹果");
    expect(
      await screen.findByText("收藏夹还没有单词。练习时可以收藏当前单词。"),
    ).toBeInTheDocument();
    await user.click(screen.getByRole("button", { name: "返回练习设置" }));
    expect(
      screen.getByRole("heading", { name: "练习设置" }),
    ).toBeInTheDocument();
  });

  it("practices favorites with the selected count and resamples the same source on restart", async () => {
    const user = userEvent.setup();
    const service = createService({
      wordbooks: [{ id: 1, name: "基础", entryCount: 30 }],
      favorites: [{ id: 7, english: "apple", chinese: "苹果" }],
    });
    render(<App wordbookService={service} />);
    await user.click(await screen.findByLabelText("练习来源"));
    await user.click(await screen.findByRole("option", { name: "收藏夹" }));
    expect(screen.queryByLabelText("当前单词本")).not.toBeInTheDocument();
    await user.click(screen.getByLabelText("本轮单词数"));
    await user.click(await screen.findByRole("option", { name: "20 个单词" }));
    await user.click(screen.getByRole("button", { name: "开始练习" }));
    expect(service.sampleFavorites).toHaveBeenCalledWith(20);
    expect(service.sampleWordbook).not.toHaveBeenCalled();
    expect(await screen.findByText("0 / 1")).toBeInTheDocument();
    await user.type(screen.getByLabelText("英文答案"), "apple{Enter}");
    expect(screen.getByText("答对题数：1 / 1")).toBeInTheDocument();
    await user.click(screen.getByRole("button", { name: "再练一轮" }));
    expect(service.sampleFavorites).toHaveBeenCalledTimes(2);
    expect(service.sampleFavorites).toHaveBeenLastCalledWith(20);
    expect(
      await screen.findByRole("heading", { name: "苹果" }),
    ).toBeInTheDocument();
  });

  it("keeps an empty or failed favorites sample on setup with a clear error", async () => {
    const user = userEvent.setup();
    const service = createService({
      wordbooks: [{ id: 1, name: "基础", entryCount: 1 }],
    });
    render(<App wordbookService={service} />);
    await user.click(await screen.findByLabelText("练习来源"));
    await user.click(await screen.findByRole("option", { name: "收藏夹" }));
    await user.click(screen.getByRole("button", { name: "开始练习" }));
    expect(
      await screen.findByText("收藏夹还没有可练习的单词。"),
    ).toHaveAttribute("role", "alert");
    expect(screen.queryByLabelText("英文答案")).not.toBeInTheDocument();
    vi.mocked(service.sampleFavorites).mockRejectedValueOnce(
      new Error("抽样失败"),
    );
    await user.click(screen.getByRole("button", { name: "开始练习" }));
    expect(await screen.findByText("抽样失败")).toHaveAttribute(
      "role",
      "alert",
    );
    expect(
      screen.getByRole("heading", { name: "练习设置" }),
    ).toBeInTheDocument();
  });

  it("reads saved pairs again after remount and replacement of the original wordbook entry", async () => {
    const user = userEvent.setup();
    const service = createService({
      wordbooks: [{ id: 1, name: "基础", entryCount: 1 }],
      samples: { 1: [{ id: 1, english: "apple", chinese: "苹果" }] },
    });
    const view = render(<App wordbookService={service} />);
    await user.click(await screen.findByRole("button", { name: "开始练习" }));
    await user.click(
      await screen.findByRole("button", { name: "收藏这个单词" }),
    );
    await screen.findByRole("button", { name: "取消收藏" });
    view.unmount();
    // The service models persistent storage; the replacement gives the same pair a new entry ID.
    vi.mocked(service.sampleWordbook).mockResolvedValue([
      { id: 999, english: "apple", chinese: "苹果" },
    ]);
    render(<App wordbookService={service} />);
    await user.click(await screen.findByRole("button", { name: "查看收藏夹" }));
    expect(await screen.findByText("苹果")).toBeInTheDocument();
    await user.click(screen.getByRole("button", { name: "返回练习设置" }));
    await user.click(screen.getByRole("button", { name: "开始练习" }));
    expect(
      await screen.findByRole("button", { name: "取消收藏" }),
    ).toBeInTheDocument();
    expect(service.isFavorite).toHaveBeenLastCalledWith("apple", "苹果");
  });

  it("ignores a late previous-question membership response and reports failed writes without flipping state", async () => {
    const user = userEvent.setup();
    let resolveFirst: (value: boolean) => void = () => undefined;
    const firstLookup = new Promise<boolean>((resolve) => {
      resolveFirst = resolve;
    });
    const service = createService({
      wordbooks: [{ id: 1, name: "基础", entryCount: 2 }],
      samples: {
        1: [
          { id: 1, english: "apple", chinese: "苹果" },
          { id: 2, english: "book", chinese: "书" },
        ],
      },
    });
    vi.mocked(service.isFavorite).mockImplementation((english) =>
      english === "apple" ? firstLookup : Promise.resolve(false),
    );
    vi.mocked(service.addFavorite).mockRejectedValueOnce(new Error("保存失败"));
    render(<App random={stableRandom} wordbookService={service} />);
    await user.click(await screen.findByRole("button", { name: "开始练习" }));
    expect(screen.getByRole("button", { name: "收藏这个单词" })).toBeDisabled();
    await user.click(screen.getByRole("button", { name: "跳过" }));
    await user.click(screen.getByRole("button", { name: "下一题" }));
    expect(
      await screen.findByRole("heading", { name: "书" }),
    ).toBeInTheDocument();
    expect(
      await screen.findByRole("button", { name: "收藏这个单词" }),
    ).toBeEnabled();
    await act(async () => {
      resolveFirst(true);
      await firstLookup;
    });
    expect(screen.getByRole("button", { name: "收藏这个单词" })).toBeEnabled();
    expect(
      screen.queryByRole("button", { name: "取消收藏" }),
    ).not.toBeInTheDocument();
    await user.click(screen.getByRole("button", { name: "收藏这个单词" }));
    expect(await screen.findByText("保存失败")).toHaveAttribute(
      "role",
      "alert",
    );
    expect(screen.getByRole("button", { name: "收藏这个单词" })).toBeEnabled();
    expect(service.addFavorite).toHaveBeenCalledWith("book", "书");
  });
  it("reports a favorite write failure after moving to the next question", async () => {
    const user = userEvent.setup();
    let rejectWrite: (reason: Error) => void = () => undefined;
    const write = new Promise<WordEntry>((_resolve, reject) => {
      rejectWrite = reject;
    });
    const service = createService({
      wordbooks: [{ id: 1, name: "基础", entryCount: 2 }],
      samples: {
        1: [
          { id: 1, english: "apple", chinese: "苹果" },
          { id: 2, english: "book", chinese: "书" },
        ],
      },
    });
    service.addFavorite = vi.fn(async () => write);
    render(<App random={stableRandom} wordbookService={service} />);
    await user.click(await screen.findByRole("button", { name: "开始练习" }));
    await user.click(
      await screen.findByRole("button", { name: "收藏这个单词" }),
    );
    await user.click(screen.getByRole("button", { name: "跳过" }));
    await user.click(screen.getByRole("button", { name: "下一题" }));
    await act(async () => {
      rejectWrite(new Error("磁盘不可写"));
      try {
        await write;
      } catch {
        /* expected */
      }
    });
    expect(await screen.findByText("收藏操作失败：磁盘不可写")).toHaveAttribute(
      "role",
      "alert",
    );
    expect(screen.getByRole("heading", { name: "书" })).toBeInTheDocument();
  });

  it("refreshes an already-open favorites list when an earlier write finishes", async () => {
    const user = userEvent.setup();
    let finishWrite: () => void = () => undefined;
    const pending = new Promise<void>((resolve) => {
      finishWrite = resolve;
    });
    const service = createService({
      wordbooks: [{ id: 1, name: "基础", entryCount: 1 }],
      samples: { 1: [{ id: 1, english: "apple", chinese: "苹果" }] },
    });
    const add = service.addFavorite;
    service.addFavorite = vi.fn(async (english, chinese) => {
      await pending;
      return add(english, chinese);
    });
    render(<App wordbookService={service} />);
    await user.click(await screen.findByRole("button", { name: "开始练习" }));
    await user.click(
      await screen.findByRole("button", { name: "收藏这个单词" }),
    );
    await user.type(screen.getByLabelText("英文答案"), "apple{Enter}");
    await user.click(screen.getByRole("button", { name: "返回模式选择" }));
    await user.click(screen.getByRole("button", { name: "查看收藏夹" }));
    expect(
      await screen.findByText("收藏夹还没有单词。练习时可以收藏当前单词。"),
    ).toBeInTheDocument();
    await act(async () => {
      finishWrite();
      await pending;
    });
    expect(await screen.findByText("苹果")).toBeInTheDocument();
  });

  it("ignores a stale list refresh after removing a favorite during another write", async () => {
    const user = userEvent.setup();
    let finishAdd: () => void = () => undefined;
    let finishRemove: () => void = () => undefined;
    let finishStaleList: () => void = () => undefined;
    const addPending = new Promise<void>((resolve) => {
      finishAdd = resolve;
    });
    const removePending = new Promise<void>((resolve) => {
      finishRemove = resolve;
    });
    const service = createService({
      wordbooks: [{ id: 1, name: "基础", entryCount: 1 }],
      samples: { 1: [{ id: 2, english: "book", chinese: "书" }] },
      favorites: [{ id: 1, english: "apple", chinese: "苹果" }],
    });
    const add = service.addFavorite;
    service.addFavorite = vi.fn(async (english, chinese) => {
      await addPending;
      return add(english, chinese);
    });
    const remove = service.removeFavorite;
    service.removeFavorite = vi.fn(async (english, chinese) => {
      await removePending;
      return remove(english, chinese);
    });
    const list = service.listFavorites;
    let listCalls = 0;
    service.listFavorites = vi.fn(async () => {
      listCalls += 1;
      if (listCalls !== 2) return list();
      const captured = await list();
      return new Promise<WordEntry[]>((resolve) => {
        finishStaleList = () => resolve(captured);
      });
    });
    render(<App wordbookService={service} />);
    await user.click(await screen.findByRole("button", { name: "开始练习" }));
    await user.click(
      await screen.findByRole("button", { name: "收藏这个单词" }),
    );
    await user.type(screen.getByLabelText("英文答案"), "book{Enter}");
    await user.click(screen.getByRole("button", { name: "返回模式选择" }));
    await user.click(screen.getByRole("button", { name: "查看收藏夹" }));
    await screen.findByRole("button", { name: "移除 apple：苹果" });
    await user.click(screen.getByRole("button", { name: "移除 apple：苹果" }));
    await act(async () => {
      finishAdd();
      await addPending;
    });
    await waitFor(() => expect(service.listFavorites).toHaveBeenCalledTimes(2));
    await act(async () => {
      finishRemove();
      await removePending;
    });
    await waitFor(() => expect(service.listFavorites).toHaveBeenCalledTimes(3));
    await act(async () => {
      finishStaleList();
    });
    expect(await screen.findByText("书")).toBeInTheDocument();
    expect(screen.queryByText("苹果")).not.toBeInTheDocument();
  });

  it("shows a failed membership lookup without claiming an unsaved favorite", async () => {
    const user = userEvent.setup();
    const service = createService({
      wordbooks: [{ id: 1, name: "基础", entryCount: 1 }],
      samples: { 1: [{ id: 1, english: "apple", chinese: "苹果" }] },
    });
    vi.mocked(service.isFavorite).mockRejectedValueOnce(new Error("查询失败"));
    render(<App wordbookService={service} />);
    await user.click(await screen.findByRole("button", { name: "开始练习" }));
    expect(await screen.findByText("查询失败")).toHaveAttribute(
      "role",
      "alert",
    );
    expect(screen.getByRole("button", { name: "收藏这个单词" })).toBeDisabled();
    expect(service.addFavorite).not.toHaveBeenCalled();
  });
  it("records one wrong completion and practices persisted mistakes after remount", async () => {
    const user = userEvent.setup();
    const service = createService({
      wordbooks: [{ id: 1, name: "基础", entryCount: 1 }],
      samples: { 1: [{ id: 1, english: "apple", chinese: "苹果" }] },
    });
    const view = render(<App wordbookService={service} />);
    await user.click(await screen.findByRole("button", { name: "开始练习" }));
    const answer = screen.getByLabelText("英文答案");
    await user.type(answer, "bad{Enter}");
    expect(screen.getByText(/本题答错/)).toBeInTheDocument();
    const submitted = screen.getByText(
      (_, element) =>
        element?.tagName === "SPAN" && element.textContent === "你的答案：bad",
    );
    expect(submitted).toBeInTheDocument();
    expect(
      screen.getByText(
        (_, element) =>
          element?.tagName === "SPAN" &&
          element.textContent === "正确答案：apple",
      ),
    ).toBeInTheDocument();
    expect(submitted.querySelector("mark")).not.toBeNull();
    await waitFor(() => expect(service.recordMistake).toHaveBeenCalledTimes(1));
    await user.click(screen.getByRole("button", { name: "查看练习结果" }));
    expect(screen.getByText("错误次数：1")).toBeInTheDocument();
    expect(screen.getByText("答对题数：0 / 1")).toBeInTheDocument();
    view.unmount();
    render(<App wordbookService={service} />);
    await user.click(await screen.findByRole("button", { name: "查看错题本" }));
    expect(await screen.findByText("错误次数：1")).toBeInTheDocument();
    expect(screen.getByText("苹果")).toBeInTheDocument();
    await user.click(screen.getByRole("button", { name: "返回练习设置" }));
    await user.click(screen.getByLabelText("练习来源"));
    await user.click(await screen.findByRole("option", { name: "错题本" }));
    await user.click(screen.getByRole("button", { name: "开始练习" }));
    expect(service.sampleMistakes).toHaveBeenCalledWith(10);
    expect(await screen.findByText("0 / 1")).toBeInTheDocument();
    await user.type(screen.getByLabelText("英文答案"), "no{Enter}");
    await waitFor(() => expect(service.recordMistake).toHaveBeenCalledTimes(2));
    await user.click(screen.getByRole("button", { name: "查看练习结果" }));
    await user.click(screen.getByRole("button", { name: "再练一轮" }));
    expect(service.sampleMistakes).toHaveBeenCalledTimes(2);
  });

  it("does not record skips or fully revealed hints, and handles an empty mistake source", async () => {
    const user = userEvent.setup();
    const service = createService({
      wordbooks: [{ id: 1, name: "基础", entryCount: 1 }],
      samples: { 1: [{ id: 1, english: "apple", chinese: "苹果" }] },
    });
    render(<App wordbookService={service} />);
    await user.click(await screen.findByRole("button", { name: "查看错题本" }));
    expect(await screen.findByText(/错题本还没有单词/)).toBeInTheDocument();
    await user.click(screen.getByRole("button", { name: "返回练习设置" }));
    await user.click(screen.getByLabelText("练习来源"));
    await user.click(await screen.findByRole("option", { name: "错题本" }));
    await user.click(screen.getByRole("button", { name: "开始练习" }));
    expect(
      await screen.findByText("错题本还没有可练习的单词。"),
    ).toHaveAttribute("role", "alert");
    await user.click(screen.getByLabelText("练习来源"));
    await user.click(await screen.findByRole("option", { name: "单词本" }));
    await user.click(screen.getByRole("button", { name: "开始练习" }));
    await user.click(await screen.findByRole("button", { name: "跳过" }));
    expect(service.recordMistake).not.toHaveBeenCalled();
    await user.click(screen.getByRole("button", { name: "查看练习结果" }));
    await user.click(screen.getByRole("button", { name: "再练一轮" }));
    await user.click(screen.getByRole("button", { name: "显示第 1 级提示" }));
    await user.click(screen.getByRole("button", { name: "显示第 2 级提示" }));
    await user.click(
      screen.getByRole("button", { name: "显示第 3 级提示并跳过" }),
    );
    expect(service.recordMistake).not.toHaveBeenCalled();
  });

  it("reports a failed mistake write without attributing it to another question", async () => {
    const user = userEvent.setup();
    const service = createService({
      wordbooks: [{ id: 1, name: "基础", entryCount: 2 }],
      samples: {
        1: [
          { id: 1, english: "apple", chinese: "苹果" },
          { id: 2, english: "book", chinese: "书" },
        ],
      },
    });
    vi.mocked(service.recordMistake).mockRejectedValueOnce(
      new Error("保存失败"),
    );
    render(<App random={stableRandom} wordbookService={service} />);
    await user.click(await screen.findByRole("button", { name: "开始练习" }));
    await user.type(screen.getByLabelText("英文答案"), "wrong{Enter}");
    expect(service.recordMistake).toHaveBeenCalledWith("apple", "苹果");
    expect(await screen.findByText("记录错题失败：保存失败")).toHaveAttribute(
      "role",
      "alert",
    );
    expect(screen.getByText(/本题答错/)).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "下一题" })).toBeDisabled();
    expect(screen.getByRole("button", { name: "重试保存错题" })).toBeEnabled();
    await user.click(screen.getByRole("button", { name: "重试保存错题" }));
    await waitFor(() =>
      expect(screen.getByRole("button", { name: "下一题" })).toBeEnabled(),
    );
    expect(service.recordMistakeOnce).toHaveBeenCalledTimes(2);
    const calls = vi.mocked(service.recordMistakeOnce).mock.calls;
    expect(calls[0]?.[2]).toBe(calls[1]?.[2]);
  });
  it("retries an acknowledged-lost mistake write without counting the error twice", async () => {
    const user = userEvent.setup();
    const service = createService({
      wordbooks: [{ id: 1, name: "基础", entryCount: 1 }],
      samples: { 1: [{ id: 1, english: "apple", chinese: "苹果" }] },
    });
    const save = service.recordMistakeOnce;
    let first = true;
    service.recordMistakeOnce = vi.fn(
      async (english, chinese, submissionId) => {
        const record = await save(english, chinese, submissionId);
        if (first) {
          first = false;
          throw new Error("响应丢失");
        }
        return record;
      },
    );
    render(<App wordbookService={service} />);
    await user.click(await screen.findByRole("button", { name: "开始练习" }));
    await user.type(screen.getByLabelText("英文答案"), "wrong{Enter}");
    expect(
      await screen.findByText(/记录错题失败：响应丢失/),
    ).toBeInTheDocument();
    await user.click(screen.getByRole("button", { name: "重试保存错题" }));
    await waitFor(() =>
      expect(
        screen.getByRole("button", { name: "查看练习结果" }),
      ).toBeEnabled(),
    );
    expect(service.recordMistake).toHaveBeenCalledTimes(1);
    expect(vi.mocked(service.recordMistakeOnce).mock.calls[0]?.[2]).toBe(
      vi.mocked(service.recordMistakeOnce).mock.calls[1]?.[2],
    );
  });

  it("waits for pending mistake persistence before opening the mistake list", async () => {
    const user = userEvent.setup();
    const service = createService({
      wordbooks: [{ id: 1, name: "基础", entryCount: 1 }],
      samples: { 1: [{ id: 1, english: "apple", chinese: "苹果" }] },
    });
    let finishWrite: () => void = () => undefined;
    const pending = new Promise<void>((resolve) => {
      finishWrite = resolve;
    });
    const record = service.recordMistake;
    service.recordMistake = vi.fn(async (english, chinese) => {
      await pending;
      return record(english, chinese);
    });
    render(<App wordbookService={service} />);
    await user.click(await screen.findByRole("button", { name: "开始练习" }));
    await user.type(screen.getByLabelText("英文答案"), "bad{Enter}");
    expect(screen.getByText(/本题答错/)).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "查看练习结果" })).toBeDisabled();
    expect(service.completeReview).not.toHaveBeenCalled();
    await act(async () => {
      finishWrite();
      await pending;
    });
    await waitFor(() =>
      expect(
        screen.getByRole("button", { name: "查看练习结果" }),
      ).toBeEnabled(),
    );
    await user.click(screen.getByRole("button", { name: "查看练习结果" }));
    await user.click(screen.getByRole("button", { name: "返回模式选择" }));
    await user.click(screen.getByRole("button", { name: "查看错题本" }));
    expect(await screen.findByText("错误次数：1")).toBeInTheDocument();
  });
  it("retries a failed mistake list load without showing stale data", async () => {
    const user = userEvent.setup();
    const service = createService({
      wordbooks: [{ id: 1, name: "基础", entryCount: 1 }],
    });
    vi.mocked(service.listMistakes).mockRejectedValueOnce(
      new Error("读取失败"),
    );
    render(<App wordbookService={service} />);
    await user.click(await screen.findByRole("button", { name: "查看错题本" }));
    expect(await screen.findByText("读取失败")).toHaveAttribute(
      "role",
      "alert",
    );
    await user.click(screen.getByRole("button", { name: "重试加载错题本" }));
    expect(await screen.findByText(/错题本还没有单词/)).toBeInTheDocument();
    expect(service.listMistakes).toHaveBeenCalledTimes(2);
  });
  it("records a wrong Chinese answer against the same word-and-meaning pair", async () => {
    const user = userEvent.setup();
    const service = createService({
      wordbooks: [{ id: 1, name: "基础", entryCount: 1 }],
      samples: { 1: [{ id: 1, english: "apple", chinese: "苹果" }] },
    });
    render(<App wordbookService={service} />);
    await user.click(await screen.findByLabelText("练习模式"));
    await user.click(
      await screen.findByRole("option", { name: "看英文拼中文" }),
    );
    await user.click(screen.getByRole("button", { name: "开始练习" }));
    await user.type(screen.getByLabelText("中文答案"), "梨{Enter}");
    expect(service.recordMistake).toHaveBeenCalledWith("apple", "苹果");
  });
  it("keeps a failed mistake sample on setup and permits retry", async () => {
    const user = userEvent.setup();
    const service = createService({
      wordbooks: [{ id: 1, name: "基础", entryCount: 1 }],
      mistakes: [{ id: 4, english: "apple", chinese: "苹果", errorCount: 3 }],
    });
    vi.mocked(service.sampleMistakes).mockRejectedValueOnce(
      new Error("错题抽样失败"),
    );
    render(<App wordbookService={service} />);
    await user.click(await screen.findByLabelText("练习来源"));
    await user.click(await screen.findByRole("option", { name: "错题本" }));
    await user.click(screen.getByRole("button", { name: "开始练习" }));
    expect(await screen.findByText("错题抽样失败")).toHaveAttribute(
      "role",
      "alert",
    );
    expect(screen.queryByLabelText("英文答案")).not.toBeInTheDocument();
    await user.click(screen.getByRole("button", { name: "开始练习" }));
    expect(await screen.findByText("0 / 1")).toBeInTheDocument();
  });
  it("offers retry instead of loading forever when a mistake write stalls", async () => {
    const user = userEvent.setup();
    const service = createService({
      wordbooks: [{ id: 1, name: "基础", entryCount: 1 }],
      samples: { 1: [{ id: 1, english: "apple", chinese: "苹果" }] },
    });
    const record = service.recordMistake;
    let writes = 0;
    service.recordMistake = vi.fn(async (english, chinese) => {
      if (++writes === 1) return new Promise<MistakeEntry>(() => undefined);
      return record(english, chinese);
    });
    render(<App wordbookService={service} />);
    await user.click(await screen.findByRole("button", { name: "开始练习" }));
    const input = screen.getByLabelText("英文答案");
    fireEvent.change(input, { target: { value: "bad" } });
    vi.useFakeTimers();
    fireEvent.keyDown(input, { key: "Enter", code: "Enter" });
    await act(async () => {
      vi.advanceTimersByTime(10_000);
    });
    vi.useRealTimers();
    expect(screen.getByText(/错题记录尚未完成，请稍后重试/)).toHaveAttribute(
      "role",
      "alert",
    );
    expect(screen.getByRole("button", { name: "重试保存错题" })).toBeEnabled();
    expect(screen.getByText(/本题答错/)).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "查看练习结果" })).toBeDisabled();
    await user.click(screen.getByRole("button", { name: "重试保存错题" }));
    await waitFor(() =>
      expect(
        screen.getByRole("button", { name: "查看练习结果" }),
      ).toBeEnabled(),
    );
    await user.click(screen.getByRole("button", { name: "查看练习结果" }));
    await user.click(screen.getByRole("button", { name: "返回模式选择" }));
    await user.click(screen.getByRole("button", { name: "查看错题本" }));
    expect(await screen.findByText("错误次数：1")).toBeInTheDocument();
  });
  it("sends each question’s error and hint counts separately", async () => {
    const user = userEvent.setup();
    const service = createService({
      wordbooks: [{ id: 1, name: "基础", entryCount: 2 }],
      samples: {
        1: [
          { id: 1, english: "apple", chinese: "苹果" },
          { id: 2, english: "book", chinese: "书" },
        ],
      },
    });
    render(<App wordbookService={service} />);
    await user.click(await screen.findByRole("button", { name: "开始练习" }));
    expect(service.schedulePractice).toHaveBeenCalledWith({
      source: "wordbook",
      wordbookId: 1,
      limit: 10,
      mode: "zh-to-en",
    });
    const input = screen.getByLabelText("英文答案");
    await user.click(screen.getByRole("button", { name: "显示第 1 级提示" }));
    await user.click(screen.getByRole("button", { name: "显示第 2 级提示" }));
    await user.type(input, "wrong{Enter}{Enter}");
    await waitFor(() =>
      expect(screen.getByRole("button", { name: "下一题" })).toBeEnabled(),
    );
    expect(service.completeReview).not.toHaveBeenCalled();
    await user.click(screen.getByRole("button", { name: "下一题" }));
    await waitFor(() =>
      expect(service.completeReview).toHaveBeenCalledWith({
        reviewId: 1,
        errorCount: 1,
        hintCount: 2,
        skipped: false,
      }),
    );
    await user.type(await screen.findByLabelText("英文答案"), "book{Enter}");
    await waitFor(() =>
      expect(service.completeReview).toHaveBeenCalledWith({
        reviewId: 2,
        errorCount: 0,
        hintCount: 0,
        skipped: false,
      }),
    );
    expect(service.recordMistake).toHaveBeenCalledTimes(1);
  });

  it("records a third hint as a skip without a mistake", async () => {
    const user = userEvent.setup();
    const service = createService({
      wordbooks: [{ id: 1, name: "基础", entryCount: 1 }],
      samples: { 1: [{ id: 1, english: "apple", chinese: "苹果" }] },
    });
    render(<App wordbookService={service} />);
    await user.click(await screen.findByRole("button", { name: "开始练习" }));
    for (const level of [1, 2, 3]) {
      await user.click(
        screen.getByRole("button", {
          name:
            level === 3 ? "显示第 3 级提示并跳过" : `显示第 ${level} 级提示`,
        }),
      );
    }
    expect(service.completeReview).not.toHaveBeenCalled();
    await user.click(screen.getByRole("button", { name: "查看练习结果" }));
    expect(service.completeReview).toHaveBeenCalledWith({
      reviewId: 1,
      errorCount: 0,
      hintCount: 3,
      skipped: true,
    });
    expect(service.recordMistake).not.toHaveBeenCalled();
  });
  it("keeps the wrong-answer comparison on review write failure and retries without another mistake", async () => {
    const user = userEvent.setup();
    const service = createService({
      wordbooks: [{ id: 1, name: "基础", entryCount: 1 }],
      samples: { 1: [{ id: 1, english: "apple", chinese: "苹果" }] },
    });
    vi.mocked(service.completeReview).mockRejectedValueOnce(
      new Error("磁盘不可用"),
    );
    render(<App wordbookService={service} />);
    await user.click(await screen.findByRole("button", { name: "开始练习" }));
    await user.type(screen.getByLabelText("英文答案"), "aple{Enter}");
    expect(screen.getByText(/本题答错/)).toBeInTheDocument();
    expect(screen.getByText(/正确答案需核对 第 3 位「p」/)).toBeInTheDocument();
    await waitFor(() =>
      expect(
        screen.getByRole("button", { name: "查看练习结果" }),
      ).toBeEnabled(),
    );
    await user.click(screen.getByRole("button", { name: "查看练习结果" }));
    expect(await screen.findByText(/保存复习进度失败，请重试/)).toHaveAttribute(
      "role",
      "alert",
    );
    expect(screen.getByText(/本题答错/)).toBeInTheDocument();
    await user.click(screen.getByRole("button", { name: "查看练习结果" }));
    expect(await screen.findByText("错误次数：1")).toBeInTheDocument();
    expect(service.recordMistake).toHaveBeenCalledTimes(1);
    expect(service.completeReview).toHaveBeenCalledTimes(2);
    expect(vi.mocked(service.completeReview).mock.calls[0]?.[0]).toEqual(
      vi.mocked(service.completeReview).mock.calls[1]?.[0],
    );
  });

  it("keeps the question available when saving a review fails and retries it", async () => {
    const user = userEvent.setup();
    const service = createService({
      wordbooks: [{ id: 1, name: "基础", entryCount: 1 }],
      samples: { 1: [{ id: 1, english: "apple", chinese: "苹果" }] },
    });
    vi.mocked(service.completeReview).mockRejectedValueOnce(
      new Error("磁盘不可用"),
    );
    render(<App wordbookService={service} />);
    await user.click(await screen.findByRole("button", { name: "开始练习" }));
    await user.type(screen.getByLabelText("英文答案"), "apple{Enter}");
    expect(
      await screen.findByText(/保存复习进度失败，请重试：磁盘不可用/),
    ).toBeInTheDocument();
    expect(screen.getByLabelText("英文答案")).toHaveValue("apple");
    await user.click(screen.getByRole("button", { name: "提交答案" }));
    expect(await screen.findByText("答对题数：1 / 1")).toBeInTheDocument();
    expect(service.completeReview).toHaveBeenCalledTimes(2);
    expect(vi.mocked(service.completeReview).mock.calls[0]?.[0]).toEqual(
      vi.mocked(service.completeReview).mock.calls[1]?.[0],
    );
  });
  it("unblocks a stalled completion for retry with the same review token", async () => {
    const user = userEvent.setup();
    const service = createService({
      wordbooks: [{ id: 1, name: "基础", entryCount: 1 }],
      samples: { 1: [{ id: 1, english: "apple", chinese: "苹果" }] },
    });
    vi.mocked(service.completeReview)
      .mockImplementationOnce(() => new Promise<void>(() => undefined))
      .mockResolvedValue(undefined);
    render(<App wordbookService={service} />);
    await user.click(await screen.findByRole("button", { name: "开始练习" }));
    const answer = screen.getByLabelText("英文答案");
    fireEvent.change(answer, { target: { value: "apple" } });
    vi.useFakeTimers();
    fireEvent.keyDown(answer, { key: "Enter", code: "Enter" });
    await act(async () => vi.advanceTimersByTime(10_000));
    vi.useRealTimers();
    expect(screen.getByText(/复习进度尚未保存，请稍后重试/)).toHaveAttribute(
      "role",
      "alert",
    );
    await user.click(screen.getByRole("button", { name: "提交答案" }));
    expect(await screen.findByText("答对题数：1 / 1")).toBeInTheDocument();
    expect(vi.mocked(service.completeReview).mock.calls[0]?.[0]).toEqual(
      vi.mocked(service.completeReview).mock.calls[1]?.[0],
    );
  });
  it("confirms deleting only the current wordbook entry and advances without counting its mistake or skip", async () => {
    const user = userEvent.setup();
    const service = createService({
      wordbooks: [{ id: 1, name: "基础", entryCount: 2 }],
      samples: {
        1: [
          { id: 1, english: "apple", chinese: "苹果" },
          { id: 2, english: "book", chinese: "书" },
        ],
      },
    });
    render(<App wordbookService={service} />);
    await user.click(await screen.findByRole("button", { name: "开始练习" }));
    await user.type(screen.getByLabelText("英文答案"), "wrong{Enter}");
    await screen.findByText(/本题答错/);
    await waitFor(() =>
      expect(screen.getByRole("button", { name: "下一题" })).toBeEnabled(),
    );
    await user.click(
      screen.getByRole("button", { name: "从单词本删除当前单词" }),
    );
    expect(screen.getByRole("alertdialog")).toBeInTheDocument();
    await user.click(screen.getByRole("button", { name: "取消" }));
    expect(service.deleteWordbookEntry).not.toHaveBeenCalled();
    expect(screen.getByText("0 / 2")).toBeInTheDocument();
    await user.click(
      screen.getByRole("button", { name: "从单词本删除当前单词" }),
    );
    await user.click(screen.getByRole("button", { name: "删除单词" }));
    await waitFor(() =>
      expect(service.deleteWordbookEntry).toHaveBeenCalledWith(1, 1),
    );
    expect(await screen.findByText("0 / 1")).toBeInTheDocument();
    await user.type(screen.getByLabelText("英文答案"), "book{Enter}");
    expect(await screen.findByText("答对题数：1 / 1")).toBeInTheDocument();
    expect(screen.getByText("错误次数：0")).toBeInTheDocument();
    expect(service.completeReview).toHaveBeenCalledTimes(1);
    await user.click(screen.getByRole("button", { name: "再练一轮" }));
    expect(await screen.findByText("0 / 1")).toBeInTheDocument();
  });

  it("deletes a revealed skipped question without submitting a review outcome", async () => {
    const user = userEvent.setup();
    const service = createService({
      wordbooks: [{ id: 1, name: "基础", entryCount: 1 }],
      samples: { 1: [{ id: 1, english: "apple", chinese: "苹果" }] },
    });
    render(<App wordbookService={service} />);
    await user.click(await screen.findByRole("button", { name: "开始练习" }));
    await user.click(screen.getByRole("button", { name: "跳过" }));
    await user.click(
      screen.getByRole("button", { name: "从单词本删除当前单词" }),
    );
    await user.click(screen.getByRole("button", { name: "删除单词" }));
    expect(await screen.findByText("答对题数：0 / 0")).toBeInTheDocument();
    expect(screen.getByText("跳过次数：0")).toBeInTheDocument();
    expect(service.completeReview).not.toHaveBeenCalled();
  });

  it("settles zero completed questions without writing an unfinished review", async () => {
    let now = 0;
    const user = userEvent.setup();
    const service = createService({
      wordbooks: [{ id: 1, name: "基础", entryCount: 1 }],
      samples: { 1: [{ id: 1, english: "apple", chinese: "苹果" }] },
    });
    render(<App now={() => now} wordbookService={service} />);
    await user.click(await screen.findByRole("button", { name: "开始练习" }));
    await user.click(screen.getByRole("button", { name: "显示第 1 级提示" }));
    await user.type(screen.getByLabelText("英文答案"), "app");
    now = 12_500;
    await user.click(screen.getByRole("button", { name: "退出并结算" }));
    expect(
      screen.getByRole("heading", { name: "练习已提前结算" }),
    ).toBeInTheDocument();
    expect(screen.getByText("已完成题数：0")).toBeInTheDocument();
    expect(screen.getByText("答对题数：0 / 0")).toBeInTheDocument();
    expect(screen.getByText("提示次数：0")).toBeInTheDocument();
    expect(screen.getByText("总用时：0:12")).toBeInTheDocument();
    expect(service.completeReview).not.toHaveBeenCalled();
  });

  it("keeps earlier completions but excludes hints on an unfinished next question", async () => {
    const user = userEvent.setup();
    const service = createService({
      wordbooks: [{ id: 1, name: "基础", entryCount: 2 }],
      samples: {
        1: [
          { id: 1, english: "apple", chinese: "苹果" },
          { id: 2, english: "book", chinese: "书" },
        ],
      },
    });
    render(<App wordbookService={service} />);
    await user.click(await screen.findByRole("button", { name: "开始练习" }));
    await user.type(screen.getByLabelText("英文答案"), "apple{Enter}");
    expect(await screen.findByText("1 / 2")).toBeInTheDocument();
    await user.click(screen.getByRole("button", { name: "显示第 1 级提示" }));
    await user.click(screen.getByRole("button", { name: "退出并结算" }));
    expect(screen.getByText("已完成题数：1")).toBeInTheDocument();
    expect(screen.getByText("答对题数：1 / 1")).toBeInTheDocument();
    expect(screen.getByText("提示次数：0")).toBeInTheDocument();
    expect(service.completeReview).toHaveBeenCalledTimes(1);
  });

  it("saves a revealed skip before settlement and retries when saving fails", async () => {
    const user = userEvent.setup();
    const service = createService({
      wordbooks: [{ id: 1, name: "基础", entryCount: 1 }],
      samples: { 1: [{ id: 1, english: "apple", chinese: "苹果" }] },
    });
    vi.mocked(service.completeReview).mockRejectedValueOnce(
      new Error("磁盘不可用"),
    );
    render(<App wordbookService={service} />);
    await user.click(await screen.findByRole("button", { name: "开始练习" }));
    await user.click(screen.getByRole("button", { name: "跳过" }));
    await user.click(screen.getByRole("button", { name: "退出并结算" }));
    expect(
      await screen.findByText(/保存复习进度失败，请重试/),
    ).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "退出并结算" })).toBeEnabled();
    expect(screen.getByText("英文：apple")).toBeInTheDocument();
    await user.click(screen.getByRole("button", { name: "退出并结算" }));
    expect(await screen.findByText("已完成题数：1")).toBeInTheDocument();
    expect(screen.getByText("跳过次数：1")).toBeInTheDocument();
    expect(service.completeReview).toHaveBeenCalledTimes(2);
    expect(vi.mocked(service.completeReview).mock.calls[0]?.[0]).toEqual({
      reviewId: 1,
      errorCount: 0,
      hintCount: 0,
      skipped: true,
    });
  });

  it("counts the third spelling hint as a saved skip on exit", async () => {
    const user = userEvent.setup();
    const service = createService({
      wordbooks: [{ id: 1, name: "基础", entryCount: 1 }],
      samples: { 1: [{ id: 1, english: "apple", chinese: "苹果" }] },
    });
    render(<App wordbookService={service} />);
    await user.click(await screen.findByRole("button", { name: "开始练习" }));
    for (const label of [
      "显示第 1 级提示",
      "显示第 2 级提示",
      "显示第 3 级提示并跳过",
    ]) {
      await user.click(screen.getByRole("button", { name: label }));
    }
    await user.click(screen.getByRole("button", { name: "退出并结算" }));
    expect(await screen.findByText("已完成题数：1")).toBeInTheDocument();
    expect(screen.getByText("提示次数：3")).toBeInTheDocument();
    expect(screen.getByText("跳过次数：1")).toBeInTheDocument();
    expect(service.completeReview).toHaveBeenCalledWith({
      reviewId: 1,
      errorCount: 0,
      hintCount: 3,
      skipped: true,
    });
  });

  it("waits for a revealed wrong answer's mistake and review writes before exiting", async () => {
    const user = userEvent.setup();
    const service = createService({
      wordbooks: [{ id: 1, name: "基础", entryCount: 1 }],
      samples: { 1: [{ id: 1, english: "apple", chinese: "苹果" }] },
    });
    let releaseMistake: () => void = () => undefined;
    const pendingMistake = new Promise<void>((resolve) => {
      releaseMistake = resolve;
    });
    let releaseReview: () => void = () => undefined;
    const pendingReview = new Promise<void>((resolve) => {
      releaseReview = resolve;
    });
    const record = service.recordMistakeOnce;
    service.recordMistakeOnce = vi.fn(
      async (english: string, chinese: string, submissionId: string) => {
        await pendingMistake;
        return record(english, chinese, submissionId);
      },
    );
    service.completeReview = vi.fn(async () => {
      await pendingReview;
    });
    render(<App wordbookService={service} />);
    await user.click(await screen.findByRole("button", { name: "开始练习" }));
    await user.type(screen.getByLabelText("英文答案"), "bad{Enter}");
    expect(screen.getByRole("button", { name: "退出并结算" })).toBeDisabled();
    await act(async () => {
      releaseMistake();
      await pendingMistake;
    });
    await waitFor(() =>
      expect(screen.getByRole("button", { name: "退出并结算" })).toBeEnabled(),
    );
    await user.click(screen.getByRole("button", { name: "退出并结算" }));
    expect(service.completeReview).toHaveBeenCalledWith({
      reviewId: 1,
      errorCount: 1,
      hintCount: 0,
      skipped: false,
    });
    expect(screen.getByRole("button", { name: "退出并结算" })).toBeDisabled();
    await act(async () => {
      releaseReview();
      await pendingReview;
    });
    expect(await screen.findByText("已完成题数：1")).toBeInTheDocument();
    expect(screen.getByText("错误次数：1")).toBeInTheDocument();
    expect(service.completeReview).toHaveBeenCalledTimes(1);
  });

  it("keeps a deleted question on failure and hides deletion outside wordbook practice", async () => {
    const user = userEvent.setup();
    const service = createService({
      wordbooks: [{ id: 1, name: "基础", entryCount: 1 }],
      samples: { 1: [{ id: 1, english: "apple", chinese: "苹果" }] },
      favorites: [{ id: 21, english: "apple", chinese: "苹果" }],
    });
    vi.mocked(service.deleteWordbookEntry).mockRejectedValueOnce(
      new Error("磁盘不可写"),
    );
    render(<App wordbookService={service} />);
    await user.click(await screen.findByRole("button", { name: "开始练习" }));
    await user.click(
      screen.getByRole("button", { name: "从单词本删除当前单词" }),
    );
    await user.click(screen.getByRole("button", { name: "删除单词" }));
    expect(await screen.findByText("删除失败：磁盘不可写")).toHaveAttribute(
      "role",
      "alert",
    );
    expect(screen.getByText("0 / 1")).toBeInTheDocument();
    await user.click(
      screen.getByRole("button", { name: "从单词本删除当前单词" }),
    );
    await user.click(screen.getByRole("button", { name: "删除单词" }));
    expect(await screen.findByText("答对题数：0 / 0")).toBeInTheDocument();
    await user.click(screen.getByRole("button", { name: "返回模式选择" }));
    await user.click(screen.getByRole("combobox", { name: "练习来源" }));
    await user.click(await screen.findByRole("option", { name: "收藏夹" }));
    await user.click(screen.getByRole("button", { name: "开始练习" }));
    expect(await screen.findByText("0 / 1")).toBeInTheDocument();
    expect(
      screen.queryByRole("button", { name: "从单词本删除当前单词" }),
    ).not.toBeInTheDocument();
  });
});

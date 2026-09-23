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
}) {
  let wordbooks = options?.wordbooks ?? [];
  const samples = options?.samples ?? {};
  let favorites = [...(options?.favorites ?? [])];
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
    expect(
      await screen.findByText("这个单词本没有可练习的词条。"),
    ).toBeInTheDocument();
    await waitFor(() =>
      expect(screen.getByRole("button", { name: "开始练习" })).toBeEnabled(),
    );
  });

  it("tracks total elapsed time across retries, hints, and questions", async () => {
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
    fireEvent.click(screen.getByRole("button", { name: "显示第 1 级提示" }));

    now = 60_000;
    act(() => vi.advanceTimersByTime(1_000));
    expect(screen.getByText("本轮用时：1:00")).toBeInTheDocument();

    fireEvent.change(firstAnswer, { target: { value: "apple" } });
    fireEvent.keyDown(firstAnswer, { key: "Enter", code: "Enter" });
    expect(screen.getByText("本轮用时：1:00")).toBeInTheDocument();

    now = 65_900;
    const secondAnswer = screen.getByLabelText("英文答案");
    fireEvent.change(secondAnswer, { target: { value: "book" } });
    fireEvent.keyDown(secondAnswer, { key: "Enter", code: "Enter" });

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
    await user.click(
      await screen.findByRole("button", { name: "双向混合随机" }),
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

  it("skips the last reverse-mode question after an error", async () => {
    const user = userEvent.setup();
    const service = createService({
      wordbooks: [{ id: 1, name: "基础", entryCount: 1 }],
      samples: { 1: [{ id: 1, english: "apple", chinese: "苹果" }] },
    });
    render(<App random={stableRandom} wordbookService={service} />);
    await user.click(
      await screen.findByRole("button", { name: "看英文拼中文" }),
    );
    await user.click(screen.getByRole("button", { name: "开始练习" }));
    await user.type(screen.getByLabelText("中文答案"), "错{Enter}");
    await user.click(screen.getByRole("button", { name: "跳过" }));
    expect(screen.getByText("英文：apple")).toBeInTheDocument();
    expect(screen.getByText("中文释义：苹果")).toBeInTheDocument();
    expect(
      screen.queryByRole("heading", { name: "本轮练习完成" }),
    ).not.toBeInTheDocument();
    await user.click(screen.getByRole("button", { name: "查看练习结果" }));
    expect(screen.getByText("答对题数：0 / 1")).toBeInTheDocument();
    expect(screen.getByText("错误次数：1")).toBeInTheDocument();
    expect(screen.getByText("跳过次数：1")).toBeInTheDocument();
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
    expect(screen.getByLabelText("当前单词本")).toBeDisabled();
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
});

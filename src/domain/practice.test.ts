import { describe, expect, it } from "vitest";
import {
  createEnglishHint,
  createQuestions,
  isExactAnswer,
  type Question,
  type RandomSource,
} from "./practice";
import type { WordEntry } from "./word";

function constantRandom(value: number): RandomSource {
  return () => value;
}

function question(direction: Question["direction"]): Question {
  return {
    id: "1",
    direction,
    entry: { id: 1, english: "house", chinese: "房子" },
  };
}

describe("createQuestions", () => {
  const entries: WordEntry[] = [
    { id: 1, english: "apple", chinese: "苹果" },
    { id: 2, english: "book", chinese: "书" },
    { id: 3, english: "cat", chinese: "猫" },
  ];

  it("covers every supplied entry exactly once in a selected direction", () => {
    const questions = createQuestions(entries, "zh-to-en", constantRandom(0.9));

    expect(questions).toHaveLength(3);
    expect(new Set(questions.map(({ id }) => id))).toEqual(
      new Set(entries.map(({ id }) => String(id))),
    );
    expect(questions.every(({ direction }) => direction === "zh-to-en")).toBe(
      true,
    );
  });

  it("supports small and ten-entry samples without a fixed-size assumption", () => {
    expect(createQuestions(entries.slice(0, 2), "zh-to-en")).toHaveLength(2);
    const tenEntries = Array.from({ length: 10 }, (_, index) => ({
      id: index + 1,
      english: `word-${index}`,
      chinese: `释义-${index}`,
    }));
    expect(createQuestions(tenEntries, "en-to-zh")).toHaveLength(10);
  });

  it("chooses both mixed directions from the injected random source", () => {
    const values = [0.9, 0.9, 0.1, 0.9, 0.1];
    let index = 0;
    const questions = createQuestions(entries, "mixed", () => values[index++]);

    expect(questions.map(({ direction }) => direction)).toContain("zh-to-en");
    expect(questions.map(({ direction }) => direction)).toContain("en-to-zh");
  });
});

describe("isExactAnswer", () => {
  it("ignores English case and surrounding whitespace only", () => {
    const englishQuestion = question("zh-to-en");

    expect(isExactAnswer("  HOUSE  ", englishQuestion)).toBe(true);
    expect(isExactAnswer("hou se", englishQuestion)).toBe(false);
    expect(isExactAnswer("houses", englishQuestion)).toBe(false);
  });

  it("requires the configured Chinese meaning after trimming", () => {
    const chineseQuestion = question("en-to-zh");

    expect(isExactAnswer(" 房子 ", chineseQuestion)).toBe(true);
    expect(isExactAnswer("住宅", chineseQuestion)).toBe(false);
    expect(isExactAnswer("", chineseQuestion)).toBe(false);
  });
});

describe("createEnglishHint", () => {
  it("forces at least one blank when randomness would reveal every letter", () => {
    const hint = createEnglishHint("apple", constantRandom(0.9));

    expect(hint).toContain("_");
    expect(hint.split(" ").join("")).toHaveLength(5);
  });

  it("keeps at least one visible letter when randomness would blank every letter", () => {
    const hint = createEnglishHint("apple", constantRandom(0));
    const compactHint = hint.split(" ").join("");

    expect(compactHint).toContain("_");
    expect(compactHint).toMatch(/[a-z]/i);
  });
});

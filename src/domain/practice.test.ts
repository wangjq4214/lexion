import { describe, expect, it } from "vitest";
import {
  createEnglishHints,
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

describe("createEnglishHints", () => {
  it("reveals approximately one and then two thirds without hiding letters again", () => {
    const [first, second] = createEnglishHints("abcdef", constantRandom(0));
    const firstLetters = first.split(" ");
    const secondLetters = second.split(" ");

    expect(firstLetters.filter((letter) => letter !== "_")).toHaveLength(2);
    expect(secondLetters.filter((letter) => letter !== "_")).toHaveLength(4);
    expect(secondLetters).toContain("_");
    firstLetters.forEach((letter, index) => {
      if (letter !== "_") expect(secondLetters[index]).toBe(letter);
    });
  });

  it("keeps short words incomplete until the third hint", () => {
    expect(createEnglishHints("a", constantRandom(0))).toEqual(["_", "_"]);
    const hints = createEnglishHints("ab", constantRandom(0));
    expect(hints[0]).toContain("_");
    expect(hints[1]).toBe(hints[0]);
  });

  it("preserves non-letters and masks only English letters", () => {
    const [first, second] = createEnglishHints("a-b c", constantRandom(0));
    expect(
      first.split(" ").filter((character) => character === "-"),
    ).toHaveLength(1);
    expect(
      second.split(" ").filter((character) => character === "_"),
    ).toHaveLength(1);
    expect(first.split(" ").join("")).toHaveLength(4);
  });
});

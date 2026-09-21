import { describe, expect, it } from "vitest";
import { WORDS } from "../data/vocabulary";
import {
  createEnglishHint,
  createQuestions,
  isExactAnswer,
  type Question,
  type RandomSource,
} from "./practice";

function constantRandom(value: number): RandomSource {
  return () => value;
}

function question(direction: Question["direction"]): Question {
  return {
    id: "house",
    direction,
    entry: { id: "house", english: "house", chinese: "房子" },
  };
}

describe("createQuestions", () => {
  it("covers every fixed word exactly once in a selected direction", () => {
    const questions = createQuestions(WORDS, "zh-to-en", constantRandom(0.9));

    expect(questions).toHaveLength(10);
    expect(new Set(questions.map(({ id }) => id))).toEqual(
      new Set(WORDS.map(({ id }) => id)),
    );
    expect(questions.every(({ direction }) => direction === "zh-to-en")).toBe(
      true,
    );
  });

  it("chooses both mixed directions from the injected random source", () => {
    const values = [
      ...Array.from({ length: WORDS.length - 1 }, () => 0.9),
      0.1,
      0.9,
      0.1,
      0.9,
      0.1,
      0.9,
      0.1,
      0.9,
      0.1,
      0.9,
    ];
    let index = 0;
    const questions = createQuestions(WORDS, "mixed", () => values[index++]);

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

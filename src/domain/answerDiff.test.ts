import { describe, expect, it } from "vitest";
import { diffAnswers } from "./answerDiff";

describe("diffAnswers", () => {
  it.each([
    ["aple", "apple", ["aple"], ["ap", "p", "le"]],
    ["appxle", "apple", ["app", "x", "le"], ["apple"]],
    ["pear", "apple", ["pe", "ar"], ["a", "p", "pl", "e"]],
    ["苹", "苹果", ["苹"], ["苹", "果"]],
    ["", "苹果", [], ["苹果"]],
    ["😀字", "😀子", ["😀", "字"], ["😀", "子"]],
  ])("aligns %s against %s", (submitted, expected, left, right) => {
    const diff = diffAnswers(submitted, expected);
    expect(diff.submitted.map((part) => part.text)).toEqual(left);
    expect(diff.expected.map((part) => part.text)).toEqual(right);
    expect(diff.submitted.map((part) => part.text).join("")).toBe(submitted);
    expect(diff.expected.map((part) => part.text).join("")).toBe(expected);
    expect(
      diff.submitted.some((part) => part.changed) ||
        diff.expected.some((part) => part.changed),
    ).toBe(true);
  });

  it("marks an insertion on the correct side without marking the shared suffix", () => {
    expect(diffAnswers("aple", "apple")).toEqual({
      submitted: [{ text: "aple", changed: false }],
      expected: [
        { text: "ap", changed: false },
        { text: "p", changed: true },
        { text: "le", changed: false },
      ],
    });
  });

  it("marks an extra letter on the submitted side without marking the shared suffix", () => {
    expect(diffAnswers("appxle", "apple")).toEqual({
      submitted: [
        { text: "app", changed: false },
        { text: "x", changed: true },
        { text: "le", changed: false },
      ],
      expected: [{ text: "apple", changed: false }],
    });
  });

  it("bounds work for very long unrelated answers and keeps their shared edges", () => {
    const prefix = "同".repeat(500);
    const suffix = "尾".repeat(500);
    const diff = diffAnswers(
      `${prefix}${"错".repeat(10_000)}${suffix}`,
      `${prefix}${"对".repeat(10_000)}${suffix}`,
    );
    expect(diff.submitted).toEqual([
      { text: prefix, changed: false },
      { text: "错".repeat(10_000), changed: true },
      { text: suffix, changed: false },
    ]);
    expect(diff.expected).toEqual([
      { text: prefix, changed: false },
      { text: "对".repeat(10_000), changed: true },
      { text: suffix, changed: false },
    ]);
  });

  it("preserves whitespace and case in the original answer", () => {
    const diff = diffAnswers(" Apple ", "apple");
    expect(diff.submitted.map((part) => part.text).join("")).toBe(" Apple ");
    expect(diff.expected.map((part) => part.text).join("")).toBe("apple");
  });
});

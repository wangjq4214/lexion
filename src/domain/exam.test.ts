import { describe, expect, it } from "vitest";
import { type ExamItem, gradeExam } from "./exam";

const questions: ExamItem[] = [
  {
    entry: { id: 1, english: "apple", chinese: "苹果" },
    direction: "en-to-zh",
  },
  { entry: { id: 2, english: "book", chinese: "书" }, direction: "zh-to-en" },
  { entry: { id: 3, english: "pear", chinese: "梨" }, direction: "en-to-zh" },
];

describe("exam grading", () => {
  it("checks all answers with practice matching and equally weighted score", () => {
    const grade = gradeExam(questions, [" 苹果 ", "BOOK", "李"]);
    expect(grade.results.map(({ correct }) => correct)).toEqual([
      true,
      true,
      false,
    ]);
    expect(grade.results[2].expected).toBe("梨");
    expect(grade.correctCount).toBe(2);
    expect(grade.score).toBeCloseTo(200 / 3);
  });

  it("counts blanks as wrong and does not accept approximate Chinese meanings", () => {
    const grade = gradeExam(questions, ["", " ", "水果"]);
    expect(grade.correctCount).toBe(0);
    expect(grade.score).toBe(0);
  });

  it("requires a nonempty exam and one answer slot per question", () => {
    expect(() => gradeExam([], [])).toThrow();
    expect(() => gradeExam(questions, ["苹果"])).toThrow();
  });
});

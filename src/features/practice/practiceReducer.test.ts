import { describe, expect, it } from "vitest";
import type { Question } from "../../domain/practice";
import { appReducer, initialState } from "./practiceReducer";

const questions: Question[] = [
  {
    id: "1",
    reviewId: 1,
    entry: { id: 11, english: "apple", chinese: "苹果" },
    direction: "zh-to-en",
  },
  {
    id: "2",
    reviewId: 2,
    entry: { id: 12, english: "book", chinese: "书" },
    direction: "zh-to-en",
  },
];

function start(entries = questions) {
  return appReducer(initialState, {
    type: "start",
    mode: "zh-to-en",
    questions: entries,
  });
}

describe("deleting a practice question", () => {
  it("removes the active question, rolls back its errors and skip, but retains used hints", () => {
    let state = start();
    state = appReducer(state, { type: "change-answer", answer: "wrong" });
    state = appReducer(state, { type: "submit-answer", elapsedSeconds: 1 });
    state = appReducer(state, {
      type: "start-hints",
      hints: ["a _ _ _ _", "a p _ _ _"],
    });
    state = appReducer(state, { type: "skip-question" });
    state = appReducer(state, {
      type: "delete-question",
      reviewId: 1,
      elapsedSeconds: 4,
    });
    expect(state).toMatchObject({
      phase: "practice",
      questionIndex: 0,
      questions: [questions[1]],
      errorCount: 0,
      questionErrorCount: 0,
      hintCount: 1,
      skippedCount: 0,
      answer: "",
      isAnswerRevealed: false,
    });
  });

  it("finishes on the last question, including the only question, without counting it", () => {
    const state = appReducer(start([questions[0]]), {
      type: "delete-question",
      reviewId: 1,
      elapsedSeconds: 9,
    });
    expect(state).toMatchObject({
      phase: "summary",
      correctCount: 0,
      totalCount: 0,
      errorCount: 0,
      skippedCount: 0,
      elapsedSeconds: 9,
    });
  });

  it("keeps earlier completed counts when deleting a later revealed question", () => {
    let state = start();
    state = appReducer(state, { type: "change-answer", answer: "apple" });
    state = appReducer(state, { type: "submit-answer", elapsedSeconds: 1 });
    state = appReducer(state, { type: "skip-question" });
    state = appReducer(state, {
      type: "delete-question",
      reviewId: 2,
      elapsedSeconds: 7,
    });
    expect(state).toMatchObject({
      phase: "summary",
      totalCount: 1,
      correctCount: 1,
      skippedCount: 0,
      elapsedSeconds: 7,
    });
  });

  it("ignores a stale review identity", () => {
    const state = start();
    expect(
      appReducer(state, {
        type: "delete-question",
        reviewId: 2,
        elapsedSeconds: 4,
      }),
    ).toBe(state);
  });
});

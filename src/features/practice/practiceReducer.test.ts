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

describe("early settlement", () => {
  it("exits the first unanswered question without counting its hints", () => {
    let state = start();
    state = appReducer(state, { type: "start-hints", hints: ["a _", "a p _"] });
    state = appReducer(state, { type: "change-answer", answer: "a" });
    state = appReducer(state, { type: "exit-and-settle", elapsedSeconds: 12 });
    expect(state).toMatchObject({
      phase: "summary",
      exitedEarly: true,
      totalCount: 0,
      correctCount: 0,
      errorCount: 0,
      hintCount: 0,
      skippedCount: 0,
      elapsedSeconds: 12,
    });
  });

  it("keeps only completed questions when leaving a later unanswered question", () => {
    let state = start();
    state = appReducer(state, { type: "change-answer", answer: "apple" });
    state = appReducer(state, { type: "submit-answer", elapsedSeconds: 3 });
    state = appReducer(state, { type: "start-hints", hints: ["b _", "b o _"] });
    state = appReducer(state, { type: "exit-and-settle", elapsedSeconds: 9 });
    expect(state).toMatchObject({
      phase: "summary",
      totalCount: 1,
      correctCount: 1,
      hintCount: 0,
      errorCount: 0,
      skippedCount: 0,
      elapsedSeconds: 9,
    });
  });

  it("includes a revealed wrong answer or skip as completed", () => {
    let wrong = start();
    wrong = appReducer(wrong, { type: "change-answer", answer: "bad" });
    wrong = appReducer(wrong, { type: "submit-answer", elapsedSeconds: 1 });
    expect(
      appReducer(wrong, { type: "exit-and-settle", elapsedSeconds: 5 }),
    ).toMatchObject({
      phase: "summary",
      totalCount: 1,
      errorCount: 1,
      skippedCount: 0,
    });
    let skipped = start();
    skipped = appReducer(skipped, { type: "skip-question" });
    expect(
      appReducer(skipped, { type: "exit-and-settle", elapsedSeconds: 5 }),
    ).toMatchObject({
      phase: "summary",
      totalCount: 1,
      errorCount: 0,
      skippedCount: 1,
    });
  });
});

describe("wrong answer reveal", () => {
  it("keeps the submitted answer and hints until the learner continues", () => {
    let state = start();
    state = appReducer(state, {
      type: "start-hints",
      hints: ["a _ _ _ _", "a p _ _ _"],
    });
    state = appReducer(state, { type: "change-answer", answer: "aple" });
    state = appReducer(state, { type: "submit-answer", elapsedSeconds: 1 });
    expect(state).toMatchObject({
      answer: "aple",
      revealReason: "wrong",
      errorCount: 1,
      questionErrorCount: 1,
      hintCount: 1,
      skippedCount: 0,
      correctCount: 0,
    });
    expect(appReducer(state, { type: "change-answer", answer: "apple" })).toBe(
      state,
    );
    expect(appReducer(state, { type: "advance-hint" })).toBe(state);
    state = appReducer(state, {
      type: "continue-after-reveal",
      elapsedSeconds: 5,
    });
    expect(state).toMatchObject({
      phase: "practice",
      questionIndex: 1,
      answer: "",
      revealReason: null,
      errorCount: 1,
      skippedCount: 0,
      correctCount: 0,
    });
  });

  it("waits for continuation on the last wrong answer", () => {
    let state = start([questions[0]]);
    state = appReducer(state, { type: "change-answer", answer: "" });
    state = appReducer(state, { type: "submit-answer", elapsedSeconds: 1 });
    expect(state.phase).toBe("practice");
    state = appReducer(state, {
      type: "continue-after-reveal",
      elapsedSeconds: 9,
    });
    expect(state).toMatchObject({
      phase: "summary",
      totalCount: 1,
      correctCount: 0,
      errorCount: 1,
      skippedCount: 0,
      elapsedSeconds: 9,
    });
  });
});

describe("deleting a practice question", () => {
  it("rolls back wrong-question errors without treating it as a skip", () => {
    let state = start();
    state = appReducer(state, { type: "change-answer", answer: "wrong" });
    state = appReducer(state, { type: "submit-answer", elapsedSeconds: 1 });
    expect(state).toMatchObject({ revealReason: "wrong", hintCount: 0 });
    expect(
      appReducer(state, { type: "submit-answer", elapsedSeconds: 2 }),
    ).toBe(state);
    expect(appReducer(state, { type: "skip-question" })).toBe(state);
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
      hintCount: 0,
      skippedCount: 0,
      answer: "",
      revealReason: null,
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

import {
  isExactAnswer,
  type PracticeMode,
  type Question,
} from "../../domain/practice";

export type SetupState = {
  phase: "setup";
  mode: PracticeMode;
};

export type PracticeState = {
  phase: "practice";
  mode: PracticeMode;
  questions: Question[];
  questionIndex: number;
  answer: string;
  errorCount: number;
  questionErrorCount: number;
  questionHintCount: number;
  hintCount: number;
  correctCount: number;
  skippedCount: number;
  revealReason: "wrong" | "skip" | null;
  hintLevel: 0 | 1 | 2;
  hints: [string, string] | null;
};

export type SummaryState = {
  phase: "summary";
  mode: PracticeMode;
  correctCount: number;
  totalCount: number;
  errorCount: number;
  hintCount: number;
  skippedCount: number;
  elapsedSeconds: number;
};

export type AppState = SetupState | PracticeState | SummaryState;

export type AppAction =
  | { type: "select-mode"; mode: PracticeMode }
  | { type: "start"; mode: PracticeMode; questions: Question[] }
  | { type: "change-answer"; answer: string }
  | { type: "submit-answer"; elapsedSeconds: number }
  | { type: "start-hints"; hints: [string, string] }
  | { type: "advance-hint" }
  | { type: "skip-question" }
  | { type: "continue-after-reveal"; elapsedSeconds: number }
  | { type: "delete-question"; reviewId: number; elapsedSeconds: number }
  | { type: "back-to-setup" };

export const initialState: AppState = {
  phase: "setup",
  mode: "zh-to-en",
};

export function appReducer(state: AppState, action: AppAction): AppState {
  if (action.type === "select-mode") {
    return state.phase === "setup" ? { ...state, mode: action.mode } : state;
  }

  if (action.type === "start") {
    return {
      phase: "practice",
      mode: action.mode,
      questions: action.questions,
      questionIndex: 0,
      answer: "",
      errorCount: 0,
      questionErrorCount: 0,
      questionHintCount: 0,
      hintCount: 0,
      correctCount: 0,
      skippedCount: 0,
      revealReason: null,
      hintLevel: 0,
      hints: null,
    };
  }

  if (action.type === "back-to-setup") {
    return { phase: "setup", mode: state.mode };
  }

  if (state.phase !== "practice") {
    return state;
  }

  if (action.type === "delete-question") {
    const question = state.questions[state.questionIndex];
    if (question.reviewId !== action.reviewId) return state;
    const questions = state.questions.filter(
      (_, index) => index !== state.questionIndex,
    );
    const errorCount = state.errorCount - state.questionErrorCount;
    const skippedCount =
      state.skippedCount - Number(state.revealReason === "skip");
    if (state.questionIndex === questions.length) {
      return {
        phase: "summary",
        mode: state.mode,
        correctCount: state.correctCount,
        totalCount: questions.length,
        errorCount,
        hintCount: state.hintCount,
        skippedCount,
        elapsedSeconds: action.elapsedSeconds,
      };
    }
    return {
      ...state,
      questions,
      answer: "",
      errorCount,
      questionErrorCount: 0,
      questionHintCount: 0,
      skippedCount,
      revealReason: null,
      hintLevel: 0,
      hints: null,
    };
  }

  if (action.type === "change-answer") {
    return state.revealReason !== null
      ? state
      : { ...state, answer: action.answer };
  }

  if (action.type === "start-hints") {
    if (
      state.revealReason !== null ||
      state.hintLevel !== 0 ||
      state.questions[state.questionIndex].direction !== "zh-to-en"
    )
      return state;
    return {
      ...state,
      hints: action.hints,
      hintLevel: 1,
      hintCount: state.hintCount + 1,
      questionHintCount: state.questionHintCount + 1,
    };
  }

  if (action.type === "advance-hint") {
    if (state.revealReason !== null || !state.hints || state.hintLevel === 0)
      return state;
    if (state.hintLevel === 1) {
      return {
        ...state,
        hintLevel: 2,
        hintCount: state.hintCount + 1,
        questionHintCount: state.questionHintCount + 1,
      };
    }
    return {
      ...state,
      revealReason: "skip",
      hintCount: state.hintCount + 1,
      questionHintCount: state.questionHintCount + 1,
      skippedCount: state.skippedCount + 1,
    };
  }

  if (action.type === "skip-question") {
    return state.revealReason !== null
      ? state
      : {
          ...state,
          revealReason: "skip",
          skippedCount: state.skippedCount + 1,
        };
  }

  if (
    action.type === "submit-answer" ||
    action.type === "continue-after-reveal"
  ) {
    if (action.type === "continue-after-reveal" && state.revealReason === null)
      return state;
    if (action.type === "submit-answer") {
      if (state.revealReason !== null) return state;
      const currentQuestion = state.questions[state.questionIndex];
      if (!isExactAnswer(state.answer, currentQuestion)) {
        return {
          ...state,
          revealReason: "wrong",
          errorCount: state.errorCount + 1,
          questionErrorCount: state.questionErrorCount + 1,
        };
      }
    }

    const correctCount =
      state.correctCount + (action.type === "submit-answer" ? 1 : 0);
    if (state.questionIndex === state.questions.length - 1) {
      return {
        phase: "summary",
        mode: state.mode,
        correctCount,
        totalCount: state.questions.length,
        errorCount: state.errorCount,
        elapsedSeconds: action.elapsedSeconds,
        hintCount: state.hintCount,
        skippedCount: state.skippedCount,
      };
    }

    return {
      ...state,
      correctCount,
      questionErrorCount: 0,
      questionHintCount: 0,
      questionIndex: state.questionIndex + 1,
      answer: "",
      hintLevel: 0,
      hints: null,
      revealReason: null,
    };
  }

  return state;
}

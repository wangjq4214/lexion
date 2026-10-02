import { render, screen } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";
import { PracticeQuestion } from "./PracticeQuestion";
import { appReducer, initialState } from "./practiceReducer";

const question = {
  id: "1",
  direction: "en-to-zh" as const,
  entry: { id: 1, english: "apple", chinese: "苹果" },
};

function renderQuestion(revealReason: "skip" | "wrong" | null) {
  const started = appReducer(initialState, {
    type: "start",
    mode: "en-to-zh",
    questions: [question],
  });
  if (started.phase !== "practice")
    throw new Error("Expected a practice round");
  render(
    <PracticeQuestion
      state={{ ...started, answer: "橘子", revealReason }}
      elapsedSeconds={0}
      isCompleting={false}
      isBlocked={false}
      onChangeAnswer={vi.fn()}
      isFavorite={false}
      isFavoriteBusy={false}
      favoriteError={null}
      onToggleFavorite={vi.fn()}
      onRetryFavorite={vi.fn()}
      isRemovingMistake={false}
      isMistakeRemoved={false}
      mistakeRemovalError={null}
      onRemoveMistake={vi.fn()}
      canDelete={false}
      isDeleting={false}
      onRequestDelete={vi.fn()}
      onHint={vi.fn()}
      onSkip={vi.fn()}
      onSubmit={vi.fn()}
      onExit={vi.fn()}
      onContinue={vi.fn()}
    />,
  );
}

describe("PracticeQuestion vocabulary typography", () => {
  it("enlarges the question prompt and revealed word and meaning", () => {
    renderQuestion("skip");
    expect(screen.getByRole("heading", { name: "apple" })).toHaveAttribute(
      "data-type",
      "display-3",
    );
    expect(screen.getByText("英文：apple")).toHaveAttribute("data-size", "xl");
    expect(screen.getByText("中文释义：苹果")).toHaveAttribute(
      "data-size",
      "xl",
    );
  });

  it("keeps the enlarged answer difference in the wrong-answer state", () => {
    renderQuestion("wrong");
    expect(screen.getByText(/你的答案：/)).toHaveAttribute("data-size", "xl");
    expect(screen.getByText(/正确答案：/)).toHaveAttribute("data-size", "xl");
  });
});

describe("PracticeQuestion mistake removal visibility", () => {
  it.each([null, "skip"] as const)(
    "does not offer removal for %s",
    (reason) => {
      renderQuestion(reason);
      expect(
        screen.queryByRole("button", { name: "从错题本移除" }),
      ).not.toBeInTheDocument();
    },
  );
  it("offers a clearly separate action after a wrong answer", () => {
    renderQuestion("wrong");
    expect(screen.getByRole("button", { name: "从错题本移除" })).toBeEnabled();
  });
});

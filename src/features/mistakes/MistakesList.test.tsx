import { render, screen } from "@testing-library/react";
import { expect, it, vi } from "vitest";
import { MistakesList } from "./MistakesList";

it("enlarges the mistake word and meaning without enlarging error metadata", () => {
  render(
    <MistakesList
      entries={[{ id: 1, english: "apple", chinese: "苹果", errorCount: 2 }]}
      error={null}
      onRetry={vi.fn()}
      backLabel="返回"
      onBack={vi.fn()}
    />,
  );
  expect(screen.getByText("apple").closest("li")).toHaveStyle({
    "--text-body-size": "var(--font-size-xl)",
  });
  expect(screen.getByText("苹果")).toHaveAttribute("data-type", "large");
  expect(screen.getByText("错误次数：2")).toHaveAttribute("data-size", "base");
});

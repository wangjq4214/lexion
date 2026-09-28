import { render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { describe, expect, it, vi } from "vitest";
import { FavoritesList } from "./FavoritesList";

describe("FavoritesList", () => {
  it("shows short remove actions while keeping each item's accessible name and target", async () => {
    const user = userEvent.setup();
    const entries = [
      { id: 1, english: "apple", chinese: "苹果" },
      { id: 2, english: "orange", chinese: "橙子" },
    ];
    const onRemove = vi.fn();

    render(
      <FavoritesList
        entries={entries}
        error={null}
        removingId={null}
        onRemove={onRemove}
        onRetry={vi.fn()}
        backLabel="返回首页"
        onBack={vi.fn()}
      />,
    );

    const appleButton = screen.getByRole("button", {
      name: "移除 apple：苹果",
    });
    const orangeButton = screen.getByRole("button", {
      name: "移除 orange：橙子",
    });
    expect(appleButton).toHaveTextContent(/^移除$/);
    expect(orangeButton).toHaveTextContent(/^移除$/);

    const appleRow = screen.getByText("apple").closest("li");
    expect(appleRow).toHaveStyle({ "--text-body-size": "var(--font-size-xl)" });
    expect(screen.getByText("苹果")).toHaveAttribute("data-type", "large");
    await user.click(orangeButton);
    expect(onRemove).toHaveBeenCalledExactlyOnceWith(entries[1]);
  });
});

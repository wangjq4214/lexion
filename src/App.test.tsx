import { render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { describe, expect, it } from "vitest";
import App from "./App";
import { WORDS } from "./data/vocabulary";

const stableRandom = () => 0.9;

describe("App vocabulary practice", () => {
  it("keeps an incorrect English answer on the current question and completes all 10 words", async () => {
    const user = userEvent.setup();
    render(<App random={stableRandom} />);

    await user.click(screen.getByRole("button", { name: "开始练习" }));

    expect(screen.getByRole("heading", { name: "苹果" })).toBeInTheDocument();
    expect(screen.getByText("0 / 10")).toBeInTheDocument();
    const firstInput = screen.getByLabelText("英文答案");
    await user.type(firstInput, "pear");
    await user.click(screen.getByRole("button", { name: "提交答案" }));

    expect(
      screen.getAllByText("答案不完全匹配，请检查后重试。"),
    ).not.toHaveLength(0);
    expect(screen.getByRole("heading", { name: "苹果" })).toBeInTheDocument();

    await user.clear(firstInput);
    await user.click(screen.getByRole("button", { name: "显示提示" }));
    expect(screen.getByText("拼写提示")).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "提示已显示" })).toBeDisabled();

    for (const entry of WORDS) {
      const input = screen.getByLabelText("英文答案");
      await user.clear(input);
      await user.type(input, entry.english.toUpperCase());
      await user.type(input, "{Enter}");
    }

    expect(
      screen.getByRole("heading", { name: "本轮练习完成" }),
    ).toBeInTheDocument();
    expect(screen.getByText("答对题数：10 / 10")).toBeInTheDocument();
    expect(screen.getByText("错误次数：1")).toBeInTheDocument();
    expect(screen.getByText("提示次数：1")).toBeInTheDocument();

    await user.click(screen.getByRole("button", { name: "再练一轮" }));
    expect(screen.getByText("0 / 10")).toBeInTheDocument();
    expect(screen.getByRole("heading", { name: "苹果" })).toBeInTheDocument();
  });

  it("does not offer an English hint when answering in Chinese", async () => {
    const user = userEvent.setup();
    render(<App random={stableRandom} />);

    await user.click(screen.getByRole("button", { name: "看英文拼中文" }));
    await user.click(screen.getByRole("button", { name: "开始练习" }));

    expect(screen.getByRole("heading", { name: "apple" })).toBeInTheDocument();
    expect(screen.getByLabelText("中文答案")).toBeInTheDocument();
    expect(
      screen.queryByRole("button", { name: "显示提示" }),
    ).not.toBeInTheDocument();
  });
});

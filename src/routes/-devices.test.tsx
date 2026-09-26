import { createMemoryHistory } from "@tanstack/react-router";
import { render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { beforeEach, expect, it, vi } from "vitest";
import App from "../App";
import { lanService } from "../data/lan";
import type { WordbookService } from "../data/wordbooks";

vi.mock("../data/lan", () => ({
  lanService: {
    status: vi.fn(),
    pair: vi.fn(),
    confirm: vi.fn(),
    cancel: vi.fn(),
  },
}));

const wordbookService = {
  listWordbooks: async () => [],
} as unknown as WordbookService;

beforeEach(() => {
  vi.mocked(lanService.status).mockReset();
  vi.mocked(lanService.confirm).mockReset();
});

it("requires the entered peer code before allowing confirmation", async () => {
  vi.mocked(lanService.status).mockResolvedValue({
    peers: [],
    trusted: [],
    pending: [{ id: "handshake-1", name: "另一台设备", code: "01234567" }],
    error: null,
  });
  vi.mocked(lanService.confirm).mockResolvedValue();
  render(
    <App
      wordbookService={wordbookService}
      history={createMemoryHistory({ initialEntries: ["/devices"] })}
    />,
  );
  const confirm = await screen.findByRole("button", {
    name: "双方数字一致，确认配对",
  });
  expect(confirm).toBeDisabled();
  await userEvent.type(
    screen.getByRole("textbox", { name: /输入.*屏幕上的八位配对码/ }),
    "99999999",
  );
  await userEvent.click(confirm);
  await waitFor(() =>
    expect(lanService.confirm).toHaveBeenCalledWith("handshake-1", "99999999"),
  );
});
it("shows per-peer synchronization results", async () => {
  vi.mocked(lanService.status).mockResolvedValue({
    peers: [],
    pending: [],
    error: null,
    trusted: [{ id: "peer-1", name: "学习设备" }],
    sync: [
      { id: "peer-1", state: "error", detail: "连接中断", last_sync: null },
    ],
  });
  render(
    <App
      wordbookService={wordbookService}
      history={createMemoryHistory({ initialEntries: ["/devices"] })}
    />,
  );
  expect(await screen.findByText(/同步失败；连接中断/)).toBeInTheDocument();
});
it("shows offline peer feedback while preserving previous sync time", async () => {
  vi.mocked(lanService.status).mockResolvedValue({
    peers: [],
    pending: [],
    error: null,
    trusted: [{ id: "peer-1", name: "学习设备" }],
    sync: [
      { id: "peer-1", state: "offline", detail: null, last_sync: "1700000000" },
    ],
  });
  render(
    <App
      wordbookService={wordbookService}
      history={createMemoryHistory({ initialEntries: ["/devices"] })}
    />,
  );
  expect(
    await screen.findByText(/设备离线，等待重连；上次同步/),
  ).toBeInTheDocument();
});

it("reports LAN unavailability without blocking local navigation", async () => {
  vi.mocked(lanService.status).mockResolvedValue({
    peers: [],
    trusted: [],
    pending: [],
    error: "mDNS 不可用",
  });
  render(
    <App
      wordbookService={wordbookService}
      history={createMemoryHistory({ initialEntries: ["/devices"] })}
    />,
  );
  expect(await screen.findByRole("alert")).toHaveTextContent(
    "本地学习不受影响",
  );
  expect(screen.getByRole("button", { name: "返回首页" })).toBeEnabled();
});

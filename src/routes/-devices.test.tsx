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
    remove: vi.fn(),
  },
}));

const wordbookService = {
  listWordbooks: async () => [],
} as unknown as WordbookService;

beforeEach(() => {
  vi.mocked(lanService.status).mockReset();
  vi.mocked(lanService.confirm).mockReset();
  vi.mocked(lanService.remove).mockReset();
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
  expect(
    await screen.findByText(/包括从其他设备同步的变更/),
  ).toBeInTheDocument();
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

it("removes only the confirmed authorized device and explains relay behavior", async () => {
  let trusted = [
    { id: "key-b", name: "设备 B" },
    { id: "key-c", name: "设备 C" },
  ];
  vi.mocked(lanService.status).mockImplementation(async () => ({
    peers: [],
    pending: [],
    trusted,
    error: null,
  }));
  vi.mocked(lanService.remove).mockImplementation(async (id) => {
    trusted = trusted.filter((peer) => peer.id !== id);
  });
  render(
    <App
      wordbookService={wordbookService}
      history={createMemoryHistory({ initialEntries: ["/devices"] })}
    />,
  );
  const user = userEvent.setup();
  await user.click(
    await screen.findByRole("button", { name: "删除与 设备 B 的配对" }),
  );
  expect(
    screen.getByText(/新变更仍可能经其他已配对设备间接同步/),
  ).toBeInTheDocument();
  expect(lanService.remove).not.toHaveBeenCalled();
  await user.click(screen.getByRole("button", { name: "保留配对" }));
  expect(lanService.remove).not.toHaveBeenCalled();
  await user.click(
    screen.getByRole("button", { name: "删除与 设备 B 的配对" }),
  );
  await user.click(screen.getByRole("button", { name: "删除直接配对" }));
  await waitFor(() =>
    expect(lanService.remove).toHaveBeenCalledExactlyOnceWith("key-b"),
  );
  expect(
    screen.queryByRole("button", { name: "删除与 设备 B 的配对" }),
  ).not.toBeInTheDocument();
  expect(
    screen.getByRole("button", { name: "删除与 设备 C 的配对" }),
  ).toBeEnabled();
});

it("reports a failed removal while leaving other pairings visible", async () => {
  vi.mocked(lanService.status).mockResolvedValue({
    peers: [],
    pending: [],
    trusted: [{ id: "key-c", name: "设备 C" }],
    error: null,
  });
  vi.mocked(lanService.remove).mockRejectedValue(new Error("无法保存配对记录"));
  render(
    <App
      wordbookService={wordbookService}
      history={createMemoryHistory({ initialEntries: ["/devices"] })}
    />,
  );
  const user = userEvent.setup();
  await user.click(
    await screen.findByRole("button", { name: "删除与 设备 C 的配对" }),
  );
  await user.click(screen.getByRole("button", { name: "删除直接配对" }));
  expect(await screen.findByRole("alert")).toHaveTextContent(
    "无法保存配对记录",
  );
  expect(
    screen.getByRole("button", { name: "删除与 设备 C 的配对" }),
  ).toBeEnabled();
});

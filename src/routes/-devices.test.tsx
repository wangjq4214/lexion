import { createMemoryHistory } from "@tanstack/react-router";
import { render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { beforeEach, expect, it, vi } from "vitest";
import App from "../App";
import { type LanStatus, lanService } from "../data/lan";
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
  vi.mocked(lanService.pair).mockReset();
  vi.mocked(lanService.cancel).mockReset();
  vi.mocked(lanService.confirm).mockReset();
  vi.mocked(lanService.remove).mockReset();
});

it("confirms matching displayed codes without typing and waits for the other device", async () => {
  let confirmed = false;
  vi.mocked(lanService.status).mockImplementation(async () => ({
    peers: [],
    trusted: [],
    pending: [
      {
        id: "handshake-1",
        peer_id: "peer-1",
        name: "另一台设备",
        code: "01234567",
        confirmed,
      },
    ],
    error: null,
  }));
  vi.mocked(lanService.confirm).mockImplementation(async () => {
    confirmed = true;
  });
  render(
    <App
      wordbookService={wordbookService}
      history={createMemoryHistory({ initialEntries: ["/devices"] })}
    />,
  );
  expect(await screen.findByText("本机配对码：01234567")).toBeInTheDocument();
  expect(screen.queryByRole("textbox")).not.toBeInTheDocument();
  await userEvent.click(
    screen.getByRole("button", { name: "两端数字一致，确认配对" }),
  );
  await waitFor(() =>
    expect(lanService.confirm).toHaveBeenCalledExactlyOnceWith("handshake-1"),
  );
  expect(
    await screen.findByText(/本机已确认，正在等待另一端确认/),
  ).toBeInTheDocument();
  expect(
    screen.queryByRole("button", { name: "两端数字一致，确认配对" }),
  ).not.toBeInTheDocument();
  expect(screen.getByRole("button", { name: "取消配对" })).toBeEnabled();
});

it("opens pairing progress immediately, then shows the code for the selected peer", async () => {
  let pending = false;
  let finishPair: () => void = () => {};
  vi.mocked(lanService.pair).mockReturnValue(
    new Promise<void>((resolve) => {
      finishPair = resolve;
    }),
  );
  vi.mocked(lanService.status).mockImplementation(async () => ({
    peers: [{ id: "peer-1", name: "学习设备", trusted: false }],
    trusted: [],
    pending: pending
      ? [
          {
            id: "request-1",
            peer_id: "peer-1",
            name: "学习设备",
            code: "01234567",
            confirmed: false,
          },
        ]
      : [],
    error: null,
  }));
  render(
    <App
      wordbookService={wordbookService}
      history={createMemoryHistory({ initialEntries: ["/devices"] })}
    />,
  );
  await userEvent.click(
    await screen.findByRole("button", { name: "与 学习设备 建立配对" }),
  );
  expect(
    screen.getByRole("alertdialog", { name: /正在与 学习设备 配对/ }),
  ).toBeInTheDocument();
  expect(screen.getByText(/正在连接设备并生成配对码/)).toBeInTheDocument();
  expect(lanService.pair).toHaveBeenCalledExactlyOnceWith("peer-1");
  pending = true;
  finishPair();
  expect(await screen.findByText("本机配对码：01234567")).toBeInTheDocument();
  await waitFor(() =>
    expect(
      screen.queryByRole("alertdialog", { name: /正在与 学习设备 配对/ }),
    ).not.toBeInTheDocument(),
  );
});

it("does not mistake another pending device with the same name for this connection", async () => {
  vi.mocked(lanService.pair).mockResolvedValue();
  vi.mocked(lanService.status).mockImplementation(async () => ({
    peers: [{ id: "peer-1", name: "学习设备", trusted: false }],
    trusted: [],
    pending: [
      {
        id: "other-session",
        peer_id: "peer-2",
        name: "学习设备",
        code: "87654321",
        confirmed: false,
      },
    ],
    error: null,
  }));
  render(
    <App
      wordbookService={wordbookService}
      history={createMemoryHistory({ initialEntries: ["/devices"] })}
    />,
  );
  await userEvent.click(
    await screen.findByRole("button", { name: "与 学习设备 建立配对" }),
  );
  expect(
    screen.getByRole("alertdialog", { name: /正在与 学习设备 配对/ }),
  ).toBeInTheDocument();
  expect(lanService.confirm).not.toHaveBeenCalled();
});

it("closes progress when a previously trusted peer reconnects without a pending code", async () => {
  let connected = false;
  vi.mocked(lanService.pair).mockImplementation(async () => {
    connected = true;
  });
  vi.mocked(lanService.status).mockImplementation(async () => ({
    peers: [{ id: "peer-1", name: "学习设备", trusted: connected }],
    trusted: connected
      ? [
          {
            id: "trusted-key",
            name: "学习设备",
            peer_id: "peer-1",
            authenticated: true,
          },
        ]
      : [],
    pending: [],
    error: null,
  }));
  render(
    <App
      wordbookService={wordbookService}
      history={createMemoryHistory({ initialEntries: ["/devices"] })}
    />,
  );
  await userEvent.click(
    await screen.findByRole("button", { name: "与 学习设备 建立配对" }),
  );
  await waitFor(() =>
    expect(screen.queryByRole("alertdialog")).not.toBeInTheDocument(),
  );
  expect(
    screen.getByRole("button", { name: "删除与 学习设备 的配对" }),
  ).toBeInTheDocument();
  expect(lanService.confirm).not.toHaveBeenCalled();
});

it("keeps progress visible while a saved pairing is still reconnecting", async () => {
  vi.mocked(lanService.pair).mockResolvedValue();
  vi.mocked(lanService.status).mockImplementation(async () => ({
    peers: [{ id: "peer-1", name: "学习设备", trusted: false }],
    trusted: [
      {
        id: "trusted-key",
        name: "学习设备",
        peer_id: "peer-1",
        authenticated: false,
      },
    ],
    pending: [],
    error: null,
  }));
  render(
    <App
      wordbookService={wordbookService}
      history={createMemoryHistory({ initialEntries: ["/devices"] })}
    />,
  );
  await userEvent.click(
    await screen.findByRole("button", { name: "与 学习设备 建立配对" }),
  );
  await waitFor(() => expect(lanService.status).toHaveBeenCalledTimes(2));
  expect(
    screen.getByRole("alertdialog", { name: /正在与 学习设备 配对/ }),
  ).toBeInTheDocument();
});

it("ignores a prior status response after a new pairing attempt starts", async () => {
  const visible: LanStatus = {
    peers: [{ id: "peer-1", name: "学习设备", trusted: false }],
    trusted: [],
    pending: [],
    error: null,
  };
  let completeOld: (value: LanStatus) => void = () => {};
  let completePair: () => void = () => {};
  vi.mocked(lanService.status)
    .mockResolvedValueOnce(visible)
    .mockImplementationOnce(
      () =>
        new Promise<LanStatus>((resolve) => {
          completeOld = resolve;
        }),
    )
    .mockResolvedValue(visible);
  vi.mocked(lanService.pair).mockReturnValue(
    new Promise<void>((resolve) => {
      completePair = resolve;
    }),
  );
  render(
    <App
      wordbookService={wordbookService}
      history={createMemoryHistory({ initialEntries: ["/devices"] })}
    />,
  );
  const user = userEvent.setup();
  await user.click(await screen.findByRole("button", { name: "刷新设备" }));
  await user.click(
    screen.getByRole("button", { name: "与 学习设备 建立配对" }),
  );
  completeOld({ ...visible, error: "previous attempt failed" });
  await waitFor(() =>
    expect(screen.getByRole("alertdialog")).toBeInTheDocument(),
  );
  expect(screen.queryByText(/previous attempt failed/)).not.toBeInTheDocument();
  completePair();
  await waitFor(() => expect(lanService.status).toHaveBeenCalledTimes(3));
});

it("uses a completed status response when a newer poll is still pending", async () => {
  const empty: LanStatus = { peers: [], pending: [], trusted: [], error: null };
  let finishEarlier: (value: LanStatus) => void = () => {};
  vi.mocked(lanService.status)
    .mockResolvedValueOnce(empty)
    .mockImplementationOnce(
      () =>
        new Promise<LanStatus>((resolve) => {
          finishEarlier = resolve;
        }),
    )
    .mockImplementationOnce(() => new Promise<LanStatus>(() => {}));
  render(
    <App
      wordbookService={wordbookService}
      history={createMemoryHistory({ initialEntries: ["/devices"] })}
    />,
  );
  await screen.findByText(/暂未发现设备/);
  const user = userEvent.setup();
  await user.click(screen.getByRole("button", { name: "刷新设备" }));
  await user.click(screen.getByRole("button", { name: "刷新设备" }));
  finishEarlier({
    ...empty,
    peers: [{ id: "peer-1", name: "学习设备", trusted: false }],
  });
  expect(
    await screen.findByRole("button", { name: "与 学习设备 建立配对" }),
  ).toBeInTheDocument();
});

it("shows a connection failure instead of leaving the pairing progress open", async () => {
  let failed = false;
  vi.mocked(lanService.pair).mockImplementation(async () => {
    failed = true;
  });
  vi.mocked(lanService.status).mockImplementation(async () => ({
    peers: [{ id: "peer-1", name: "学习设备", trusted: false }],
    trusted: [],
    pending: [],
    error: failed ? "Pairing connection failed: timeout" : null,
  }));
  render(
    <App
      wordbookService={wordbookService}
      history={createMemoryHistory({ initialEntries: ["/devices"] })}
    />,
  );
  await userEvent.click(
    await screen.findByRole("button", { name: "与 学习设备 建立配对" }),
  );
  expect(await screen.findByRole("alert")).toHaveTextContent(
    "Pairing connection failed: timeout",
  );
  await waitFor(() =>
    expect(screen.queryByRole("alertdialog")).not.toBeInTheDocument(),
  );
});

it("lets a user cancel a mismatched code without authorizing a peer", async () => {
  let pending = true;
  vi.mocked(lanService.status).mockImplementation(async () => ({
    peers: [],
    trusted: [],
    pending: pending
      ? [
          {
            id: "request-1",
            peer_id: "peer-1",
            name: "另一台设备",
            code: "01234567",
            confirmed: false,
          },
        ]
      : [],
    error: null,
  }));
  vi.mocked(lanService.cancel).mockImplementation(async () => {
    pending = false;
  });
  render(
    <App
      wordbookService={wordbookService}
      history={createMemoryHistory({ initialEntries: ["/devices"] })}
    />,
  );
  await userEvent.click(
    await screen.findByRole("button", { name: "取消配对" }),
  );
  expect(lanService.cancel).toHaveBeenCalledExactlyOnceWith("request-1");
  expect(lanService.confirm).not.toHaveBeenCalled();
  expect(
    await screen.findByText(/已取消与 另一台设备 的配对请求/),
  ).toBeInTheDocument();
});
it("shows per-peer synchronization results", async () => {
  vi.mocked(lanService.status).mockResolvedValue({
    peers: [],
    pending: [],
    error: null,
    trusted: [
      {
        id: "peer-1",
        name: "学习设备",
        peer_id: "peer-1",
        authenticated: true,
      },
    ],
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
    trusted: [
      {
        id: "peer-1",
        name: "学习设备",
        peer_id: "peer-1",
        authenticated: false,
      },
    ],
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
    { id: "key-b", name: "设备 B", peer_id: "peer-b", authenticated: false },
    { id: "key-c", name: "设备 C", peer_id: "peer-c", authenticated: false },
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
    trusted: [
      { id: "key-c", name: "设备 C", peer_id: "peer-c", authenticated: false },
    ],
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

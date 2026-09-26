import { invoke } from "@tauri-apps/api/core";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { lanService } from "./lan";

vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn() }));

beforeEach(() => vi.mocked(invoke).mockReset());

describe("LAN command contract", () => {
  it("queries discovery, pending handshakes and direct trust from Rust", async () => {
    const status = { peers: [], pending: [], trusted: [], error: null };
    vi.mocked(invoke).mockResolvedValue(status);
    expect(await lanService.status()).toEqual(status);
    expect(invoke).toHaveBeenCalledWith("lan_status");
  });

  it("passes explicit peer selection and entered code to Rust", async () => {
    await lanService.pair("peer-a");
    await lanService.confirm("session-a", "01234567");
    await lanService.cancel("session-b");
    expect(invoke).toHaveBeenNthCalledWith(1, "lan_pair", { peerId: "peer-a" });
    expect(invoke).toHaveBeenNthCalledWith(2, "lan_confirm", {
      id: "session-a",
      code: "01234567",
    });
    expect(invoke).toHaveBeenNthCalledWith(3, "lan_cancel", {
      id: "session-b",
    });
  });
});

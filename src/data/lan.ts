import { invoke } from "@tauri-apps/api/core";

export type LanPeer = { id: string; name: string; trusted: boolean };
export type PendingPair = { id: string; name: string; code: string };
export type TrustedPeer = { id: string; name: string };
export type LanStatus = {
  peers: LanPeer[];
  pending: PendingPair[];
  trusted: TrustedPeer[];
  error: string | null;
};

export const lanService = {
  status: () => invoke<LanStatus>("lan_status"),
  pair: (peerId: string) => invoke<void>("lan_pair", { peerId }),
  confirm: (id: string, code: string) =>
    invoke<void>("lan_confirm", { id, code }),
  cancel: (id: string) => invoke<void>("lan_cancel", { id }),
};

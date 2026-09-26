import { isTauri } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { useSetAtom } from "jotai";
import {
  createContext,
  type ReactNode,
  useCallback,
  useContext,
  useEffect,
} from "react";
import type { WordbookService } from "../../data/wordbooks";
import {
  activeWordbookAtom,
  syncedDataVersionAtom,
  wordbooksAtom,
  wordbooksErrorAtom,
} from "../../state/appState";

const RefreshContext = createContext<
  ((preferredId?: number) => Promise<void>) | null
>(null);
export function useRefreshWordbooks() {
  const refresh = useContext(RefreshContext);
  if (!refresh) throw new Error("WordbookProvider is missing");
  return refresh;
}

export function WordbookProvider({
  service,
  children,
}: {
  service: WordbookService;
  children: ReactNode;
}) {
  const setWordbooks = useSetAtom(wordbooksAtom);
  const setLoadError = useSetAtom(wordbooksErrorAtom);
  const setActiveId = useSetAtom(activeWordbookAtom);
  const setSyncedVersion = useSetAtom(syncedDataVersionAtom);
  const refresh = useCallback(
    async (preferredId?: number) => {
      try {
        const listed = await service.listWordbooks();
        setWordbooks(listed);
        setLoadError(null);
        setActiveId((current) =>
          current !== null && listed.some(({ id }) => id === current)
            ? current
            : preferredId !== undefined &&
                listed.some(({ id }) => id === preferredId)
              ? preferredId
              : (listed[0]?.id ?? null),
        );
      } catch (error) {
        setLoadError(error instanceof Error ? error.message : String(error));
        setWordbooks(null);
      }
    },
    [service, setWordbooks, setLoadError, setActiveId],
  );
  useEffect(() => {
    let current = true;
    void service.listWordbooks().then(
      (listed) => {
        if (!current) return;
        setWordbooks(listed);
        setLoadError(null);
        setActiveId(listed[0]?.id ?? null);
      },
      (error) => {
        if (!current) return;
        setLoadError(error instanceof Error ? error.message : String(error));
        setWordbooks(null);
      },
    );
    return () => {
      current = false;
    };
  }, [service, setWordbooks, setLoadError, setActiveId]);
  useEffect(() => {
    if (!isTauri()) return;
    let mounted = true;
    let unlisten: (() => void) | undefined;
    void listen("learning-data-synced", () => {
      if (!mounted) return;
      void refresh();
      setSyncedVersion((value) => value + 1);
    }).then((stop) => {
      if (mounted) unlisten = stop;
      else stop();
    });
    return () => {
      mounted = false;
      unlisten?.();
    };
  }, [refresh, setSyncedVersion]);
  return (
    <RefreshContext.Provider value={refresh}>
      {children}
    </RefreshContext.Provider>
  );
}

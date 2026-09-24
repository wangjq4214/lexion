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
  return (
    <RefreshContext.Provider value={refresh}>
      {children}
    </RefreshContext.Provider>
  );
}

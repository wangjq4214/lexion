import { createFileRoute, useNavigate } from "@tanstack/react-router";
import { useAtomValue } from "jotai";
import { useCallback, useEffect, useRef, useState } from "react";
import type { WordbookService } from "../data/wordbooks";
import type { WordEntry } from "../domain/word";
import { FavoritesList } from "../features/favorites/FavoritesList";
import { favoritesVersionAtom } from "../state/appState";

function FavoritesPage() {
  const { props } = Route.useRouteContext();
  const service: WordbookService = props.wordbookService;
  const version = useAtomValue(favoritesVersionAtom);
  const navigate = useNavigate();
  const fromSetup = Route.useSearch().from === "practice-setup";
  const [entries, setEntries] = useState<WordEntry[] | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [removingId, setRemovingId] = useState<number | null>(null);
  const request = useRef(0);
  const load = useCallback(() => {
    const id = ++request.current;
    setEntries(null);
    setError(null);
    void service.listFavorites().then(
      (items) => {
        if (request.current === id) setEntries(items);
      },
      (reason) => {
        if (request.current === id)
          setError(reason instanceof Error ? reason.message : String(reason));
      },
    );
  }, [service]);
  // The atom is an explicit cross-route invalidation signal for writes finishing after practice unmounts.
  // biome-ignore lint/correctness/useExhaustiveDependencies: version intentionally retriggers the request.
  useEffect(() => {
    // Defer the request so a route entered and immediately left cannot publish stale data.
    let mounted = true;
    const requestRef = request;
    queueMicrotask(() => {
      if (mounted) load();
    });
    return () => {
      mounted = false;
      requestRef.current++;
    };
  }, [load, version]);
  const remove = async (entry: WordEntry) => {
    if (removingId !== null) return;
    setRemovingId(entry.id);
    setError(null);
    try {
      await service.removeFavorite(entry.english, entry.chinese);
      load();
    } catch (reason) {
      setError(reason instanceof Error ? reason.message : String(reason));
    } finally {
      setRemovingId(null);
    }
  };
  return (
    <FavoritesList
      entries={entries}
      error={error}
      removingId={removingId}
      onRemove={remove}
      onRetry={load}
      backLabel={fromSetup ? "返回练习设置" : "返回首页"}
      onBack={() => void navigate({ to: fromSetup ? "/practice-setup" : "/" })}
    />
  );
}
export const Route = createFileRoute("/favorites")({
  validateSearch: (search: Record<string, unknown>) => ({
    from: search.from === "practice-setup" ? "practice-setup" : undefined,
  }),
  component: FavoritesPage,
});

import { createFileRoute, useNavigate } from "@tanstack/react-router";
import { useCallback, useEffect, useRef, useState } from "react";
import type { MistakeEntry, WordbookService } from "../data/wordbooks";
import { MistakesList } from "../features/mistakes/MistakesList";

function MistakesPage() {
  const { props } = Route.useRouteContext();
  const service: WordbookService = props.wordbookService;
  const navigate = useNavigate();
  const fromSetup = Route.useSearch().from === "practice-setup";
  const [entries, setEntries] = useState<MistakeEntry[] | null>(null);
  const [error, setError] = useState<string | null>(null);
  const request = useRef(0);
  const load = useCallback(() => {
    const id = ++request.current;
    setEntries(null);
    setError(null);
    void service.listMistakes().then(
      (items) => {
        if (request.current === id) setEntries(items);
      },
      (reason) => {
        if (request.current === id)
          setError(reason instanceof Error ? reason.message : String(reason));
      },
    );
  }, [service]);
  useEffect(() => {
    let mounted = true;
    const requestRef = request;
    queueMicrotask(() => {
      if (mounted) load();
    });
    return () => {
      mounted = false;
      requestRef.current++;
    };
  }, [load]);
  return (
    <MistakesList
      entries={entries}
      error={error}
      onRetry={load}
      backLabel={fromSetup ? "返回练习设置" : "返回首页"}
      onBack={() => void navigate({ to: fromSetup ? "/practice-setup" : "/" })}
    />
  );
}
export const Route = createFileRoute("/mistakes")({
  validateSearch: (search: Record<string, unknown>) => ({
    from: search.from === "practice-setup" ? "practice-setup" : undefined,
  }),
  component: MistakesPage,
});

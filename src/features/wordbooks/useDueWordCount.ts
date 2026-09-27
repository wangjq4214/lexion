import { useEffect, useState } from "react";
import type { WordbookService } from "../../data/wordbooks";

function tomorrowStartSeconds() {
  const tomorrow = new Date();
  tomorrow.setHours(24, 0, 0, 0);
  return tomorrow.getTime() / 1000;
}

export function useDueWordCount(
  service: WordbookService,
  wordbookId: number | null,
  enabled = true,
  dataVersion = 0,
) {
  const [dayVersion, setDayVersion] = useState(0);
  const [result, setResult] = useState<{
    key: number | null;
    version: number;
    dayVersion: number;
    count: number | null;
    error: string | null;
  } | null>(null);
  useEffect(() => {
    if (!enabled) return;
    const delay = tomorrowStartSeconds() * 1000 - Date.now();
    const timer = window.setTimeout(
      () => setDayVersion(dayVersion + 1),
      Math.max(1, delay),
    );
    return () => window.clearTimeout(timer);
  }, [enabled, dayVersion]);
  useEffect(() => {
    if (!enabled) return;
    let current = true;
    void service.dueWordCount(wordbookId, tomorrowStartSeconds()).then(
      (count) => {
        if (current)
          setResult({
            key: wordbookId,
            version: dataVersion,
            dayVersion,
            count,
            error: null,
          });
      },
      (error: unknown) => {
        if (current)
          setResult({
            key: wordbookId,
            version: dataVersion,
            dayVersion,
            count: null,
            error: error instanceof Error ? error.message : String(error),
          });
      },
    );
    return () => {
      current = false;
    };
  }, [service, wordbookId, enabled, dataVersion, dayVersion]);
  return enabled &&
    result?.key === wordbookId &&
    result.version === dataVersion &&
    result.dayVersion === dayVersion
    ? result
    : null;
}

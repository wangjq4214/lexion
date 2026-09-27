import { act, renderHook } from "@testing-library/react";
import { afterEach, expect, it, vi } from "vitest";
import type { WordbookService } from "../../data/wordbooks";
import { useDueWordCount } from "./useDueWordCount";

afterEach(() => vi.useRealTimers());

it("refreshes the count after a synced data version change", async () => {
  const dueWordCount = vi
    .fn()
    .mockResolvedValueOnce(2)
    .mockResolvedValueOnce(5);
  const service = { dueWordCount } as unknown as WordbookService;
  const { result, rerender, unmount } = renderHook(
    ({ version }) => useDueWordCount(service, 1, true, version),
    { initialProps: { version: 0 } },
  );
  await act(async () => {});
  expect(result.current?.count).toBe(2);
  rerender({ version: 1 });
  expect(result.current).toBeNull();
  await act(async () => {});
  expect(result.current?.count).toBe(5);
  expect(dueWordCount).toHaveBeenCalledTimes(2);
  unmount();
});

it("refreshes today's cutoff when local midnight passes", async () => {
  vi.useFakeTimers();
  vi.setSystemTime(new Date(2026, 8, 27, 23, 59, 59));
  const dueWordCount = vi
    .fn()
    .mockResolvedValueOnce(1)
    .mockResolvedValueOnce(3);
  const service = { dueWordCount } as unknown as WordbookService;
  const { result, unmount } = renderHook(() => useDueWordCount(service, null));
  await act(async () => {});
  expect(result.current?.count).toBe(1);
  const firstCutoff = dueWordCount.mock.calls[0]?.[1];
  await act(async () => {
    await vi.advanceTimersByTimeAsync(1000);
  });
  expect(result.current?.count).toBe(3);
  expect(dueWordCount).toHaveBeenCalledTimes(2);
  const nextCutoff = dueWordCount.mock.calls[1]?.[1];
  expect(nextCutoff).toBeGreaterThan(firstCutoff);
  unmount();
});

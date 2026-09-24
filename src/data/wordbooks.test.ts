import { invoke } from "@tauri-apps/api/core";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { tauriWordbookService } from "./wordbooks";

vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn() }));

beforeEach(() => {
  vi.mocked(invoke).mockReset();
});
describe("wordbook deletion service contract", () => {
  it("deletes by ID and maps database errors", async () => {
    vi.mocked(invoke).mockResolvedValueOnce(true).mockRejectedValueOnce({
      kind: "database",
      message: "删除失败",
    });
    await expect(tauriWordbookService.deleteWordbook(3)).resolves.toBe(true);
    expect(invoke).toHaveBeenCalledWith("delete_wordbook", { wordbookId: 3 });
    await expect(tauriWordbookService.deleteWordbook(3)).rejects.toMatchObject({
      kind: "database",
      message: "删除失败",
    });
  });
});

describe("review scheduling service contract", () => {
  it("exposes the adjustable retention target", async () => {
    vi.mocked(invoke)
      .mockResolvedValueOnce(0.9)
      .mockResolvedValueOnce(undefined);
    await expect(tauriWordbookService.reviewTarget()).resolves.toBe(0.9);
    await tauriWordbookService.setReviewTarget(0.8);
    expect(invoke).toHaveBeenNthCalledWith(1, "review_target");
    expect(invoke).toHaveBeenNthCalledWith(2, "set_review_target", {
      target: 0.8,
    });
  });

  it("forwards source, wordbook identity, count and mode without randomizing direction", async () => {
    const questions = [
      {
        reviewId: 4,
        entry: { id: 10, english: "apple", chinese: "苹果" },
        direction: "en-to-zh" as const,
      },
    ];
    vi.mocked(invoke).mockResolvedValueOnce(questions);

    await expect(
      tauriWordbookService.schedulePractice({
        source: "wordbook",
        wordbookId: 2,
        limit: 5,
        mode: "mixed",
      }),
    ).resolves.toEqual(questions);
    expect(invoke).toHaveBeenCalledWith("schedule_practice", {
      source: "wordbook",
      wordbookId: 2,
      limit: 5,
      mode: "mixed",
    });
  });

  it("forwards the completed question counts and maps persistence errors", async () => {
    const outcome = {
      reviewId: 4,
      errorCount: 2,
      hintCount: 1,
      skipped: false,
    };
    vi.mocked(invoke).mockResolvedValueOnce(undefined).mockRejectedValueOnce({
      kind: "database",
      message: "写入失败",
    });

    await tauriWordbookService.completeReview(outcome);
    expect(invoke).toHaveBeenCalledWith("complete_review", outcome);
    await expect(
      tauriWordbookService.completeReview(outcome),
    ).rejects.toMatchObject({
      kind: "database",
      message: "写入失败",
    });
  });
  it("passes one durable submission ID to the mistake command", async () => {
    const record = { id: 2, english: "apple", chinese: "苹果", errorCount: 1 };
    vi.mocked(invoke).mockResolvedValueOnce(record);
    await expect(
      tauriWordbookService.recordMistakeOnce("apple", "苹果", "submission-1"),
    ).resolves.toEqual(record);
    expect(invoke).toHaveBeenCalledWith("record_mistake_once", {
      english: "apple",
      chinese: "苹果",
      submissionId: "submission-1",
    });
  });
});

import { AlertDialog } from "@astryxdesign/core/AlertDialog";
import { Button } from "@astryxdesign/core/Button";
import { Stack } from "@astryxdesign/core/Stack";
import { Text } from "@astryxdesign/core/Text";
import { useToast } from "@astryxdesign/core/Toast";
import {
  createFileRoute,
  useBlocker,
  useNavigate,
} from "@tanstack/react-router";
import { useAtomValue, useSetAtom } from "jotai";
import { useEffect, useRef, useState } from "react";
import type { WordbookService } from "../data/wordbooks";
import {
  createEnglishHints,
  getExpectedAnswer,
  isExactAnswer,
} from "../domain/practice";
import type { WordEntry } from "../domain/word";
import { PracticeQuestion } from "../features/practice/PracticeQuestion";
import {
  activeWordbookAtom,
  dispatchRoundAtom,
  favoritesVersionAtom,
  favoriteWriteErrorAtom,
  practiceSourceAtom,
  roundAtom,
  roundKeyAtom,
  roundStartedAtAtom,
  wordbooksAtom,
} from "../state/appState";

type MistakeSubmission = { id: string; english: string; chinese: string };
type DeleteTarget = {
  roundKey: number;
  questionIndex: number;
  reviewId: number;
  wordbookId: number;
  entry: WordEntry;
};
const elapsed = (
  started: number | null,
  now: () => number,
  previous: number,
) =>
  started === null
    ? previous
    : Math.max(0, Math.floor((now() - started) / 1000));
function submissionId() {
  return Array.from(crypto.getRandomValues(new Uint8Array(16)), (byte) =>
    byte.toString(16).padStart(2, "0"),
  ).join("");
}
async function withWriteTimeout<T>(
  write: Promise<T>,
  message: string,
): Promise<T> {
  let timer: ReturnType<typeof setTimeout> | undefined;
  try {
    return await Promise.race([
      write,
      new Promise<never>((_, reject) => {
        timer = setTimeout(() => reject(new Error(message)), 10_000);
      }),
    ]);
  } finally {
    if (timer !== undefined) clearTimeout(timer);
  }
}

function PracticePage() {
  const { props } = Route.useRouteContext();
  const service: WordbookService = props.wordbookService;
  const now = props.now;
  const random = props.random;
  const navigate = useNavigate();
  const showToast = useToast();
  const setWordbooks = useSetAtom(wordbooksAtom);
  const state = useAtomValue(roundAtom);
  const dispatch = useSetAtom(dispatchRoundAtom);
  const setFavoritesVersion = useSetAtom(favoritesVersionAtom);
  const source = useAtomValue(practiceSourceAtom);
  const wordbookId = useAtomValue(activeWordbookAtom);
  const roundKey = useAtomValue(roundKeyAtom);
  const startedAt = useAtomValue(roundStartedAtAtom);
  const [navigationNotice, setNavigationNotice] = useState(false);
  useBlocker({
    shouldBlockFn: ({ next }) => {
      if (state.phase !== "practice" || next.pathname === "/practice")
        return false;
      setNavigationNotice(true);
      return true;
    },
  });
  const [elapsedSeconds, setElapsedSeconds] = useState(0);
  useEffect(() => {
    if (state.phase !== "practice") return;
    const interval = window.setInterval(
      () => setElapsedSeconds((previous) => elapsed(startedAt, now, previous)),
      1000,
    );
    return () => window.clearInterval(interval);
  }, [state.phase, startedAt, now]);
  useEffect(() => {
    if (state.phase === "summary")
      void navigate({ to: "/summary", replace: true });
  }, [state.phase, navigate]);

  const completingReview = useRef(false);
  const pendingMistakeRef = useRef<MistakeSubmission | null>(null);
  const savingMistakeRef = useRef(false);
  const deletingRef = useRef(false);
  const favoriteBusy = useRef(false);
  const favoriteRequest = useRef(0);
  const [pendingMistake, setPendingMistake] =
    useState<MistakeSubmission | null>(null);
  const [isSavingMistake, setIsSavingMistake] = useState(false);
  const [mistakeWriteError, setMistakeWriteError] = useState<string | null>(
    null,
  );
  const [reviewWriteError, setReviewWriteError] = useState<string | null>(null);
  const [isCompleting, setIsCompleting] = useState(false);
  const favoriteWriteError = useAtomValue(favoriteWriteErrorAtom);
  const setFavoriteWriteError = useSetAtom(favoriteWriteErrorAtom);
  const [isFavoriteBusy, setIsFavoriteBusy] = useState(false);
  const [favoriteStatus, setFavoriteStatus] = useState<{
    key: string;
    saved: boolean;
  } | null>(null);
  const [favoriteErrorState, setFavoriteErrorState] = useState<{
    key: string;
    message: string;
  } | null>(null);
  const [favoriteRefresh, setFavoriteRefresh] = useState(0);
  const [deleteTarget, setDeleteTarget] = useState<DeleteTarget | null>(null);
  const [isDeleting, setIsDeleting] = useState(false);
  const [deleteError, setDeleteError] = useState<string | null>(null);

  const currentEntry =
    state.phase === "practice"
      ? state.questions[state.questionIndex].entry
      : null;
  const favoriteQueryKey =
    state.phase === "practice"
      ? `${roundKey}:${state.questionIndex}:${favoriteRefresh}`
      : null;
  const isFavorite =
    favoriteStatus?.key === favoriteQueryKey ? favoriteStatus.saved : null;
  const favoriteError =
    favoriteErrorState?.key === favoriteQueryKey
      ? favoriteErrorState.message
      : null;
  const english = currentEntry?.english;
  const chinese = currentEntry?.chinese;
  useEffect(() => {
    const request = ++favoriteRequest.current;
    if (
      favoriteQueryKey === null ||
      english === undefined ||
      chinese === undefined
    )
      return;
    void service.isFavorite(english, chinese).then(
      (saved) => {
        if (favoriteRequest.current === request) {
          setFavoriteStatus({ key: favoriteQueryKey, saved });
          setFavoriteErrorState(null);
        }
      },
      (error) => {
        if (favoriteRequest.current === request)
          setFavoriteErrorState({
            key: favoriteQueryKey,
            message: error instanceof Error ? error.message : String(error),
          });
      },
    );
    const requestRef = favoriteRequest;
    return () => {
      requestRef.current++;
    };
  }, [favoriteQueryKey, english, chinese, service]);
  const toggleFavorite = async () => {
    if (!currentEntry || isFavorite === null || favoriteBusy.current) return;
    favoriteBusy.current = true;
    setIsFavoriteBusy(true);
    setFavoriteErrorState(null);
    setFavoriteWriteError(null);
    const request = favoriteRequest.current;
    try {
      if (isFavorite)
        await service.removeFavorite(
          currentEntry.english,
          currentEntry.chinese,
        );
      else
        await service.addFavorite(currentEntry.english, currentEntry.chinese);
      if (favoriteRequest.current === request && favoriteQueryKey !== null)
        setFavoriteStatus({ key: favoriteQueryKey, saved: !isFavorite });
      setFavoritesVersion((value) => value + 1);
      showToast({
        body: isFavorite ? "已取消" : "已收藏",
        uniqueID: "favorite-feedback",
        autoHideDuration: 2500,
      });
    } catch (error) {
      const message = error instanceof Error ? error.message : String(error);
      if (favoriteRequest.current === request && favoriteQueryKey !== null)
        setFavoriteErrorState({ key: favoriteQueryKey, message });
      setFavoriteWriteError(`收藏操作失败：${message}`);
    } finally {
      favoriteBusy.current = false;
      setIsFavoriteBusy(false);
    }
  };
  const saveMistake = async (submission: MistakeSubmission) => {
    if (savingMistakeRef.current) return;
    savingMistakeRef.current = true;
    setIsSavingMistake(true);
    setMistakeWriteError(null);
    const write = service.recordMistakeOnce(
      submission.english,
      submission.chinese,
      submission.id,
    );
    try {
      await withWriteTimeout(write, "错题记录尚未完成，请稍后重试。");
      pendingMistakeRef.current = null;
      setPendingMistake(null);
    } catch (error) {
      setMistakeWriteError(
        `记录错题失败：${error instanceof Error ? error.message : String(error)}`,
      );
    } finally {
      savingMistakeRef.current = false;
      setIsSavingMistake(false);
    }
  };
  const completeQuestion = async (
    type: "submit-answer" | "continue-after-reveal" | "exit-and-settle",
  ) => {
    if (
      state.phase !== "practice" ||
      completingReview.current ||
      deletingRef.current ||
      pendingMistakeRef.current
    )
      return;
    const currentElapsed = elapsed(startedAt, now, elapsedSeconds);
    if (type === "exit-and-settle" && state.revealReason === null) {
      setElapsedSeconds(currentElapsed);
      dispatch({ type, elapsedSeconds: currentElapsed });
      return;
    }
    const question = state.questions[state.questionIndex];
    if (question.reviewId === undefined) {
      setReviewWriteError("复习记录缺少题目身份，请重新开始练习。");
      return;
    }
    completingReview.current = true;
    setReviewWriteError(null);
    setIsCompleting(true);
    try {
      await withWriteTimeout(
        service.completeReview({
          reviewId: question.reviewId,
          errorCount: state.questionErrorCount,
          hintCount: state.questionHintCount,
          skipped: type !== "submit-answer" && state.revealReason === "skip",
        }),
        "复习进度尚未保存，请稍后重试。",
      );
      setElapsedSeconds(currentElapsed);
      dispatch({ type, elapsedSeconds: currentElapsed });
    } catch (error) {
      setReviewWriteError(
        `保存复习进度失败，请重试：${error instanceof Error ? error.message : String(error)}`,
      );
    } finally {
      completingReview.current = false;
      setIsCompleting(false);
    }
  };
  const submitAnswer = () => {
    if (
      state.phase !== "practice" ||
      state.revealReason !== null ||
      completingReview.current ||
      deletingRef.current ||
      pendingMistakeRef.current
    )
      return;
    const question = state.questions[state.questionIndex];
    if (!isExactAnswer(state.answer, question)) {
      const submission = {
        id: submissionId(),
        english: question.entry.english,
        chinese: question.entry.chinese,
      };
      pendingMistakeRef.current = submission;
      setPendingMistake(submission);
      dispatch({ type: "submit-answer", elapsedSeconds });
      void saveMistake(submission);
      return;
    }
    void completeQuestion("submit-answer");
  };
  const requestDelete = () => {
    if (
      state.phase !== "practice" ||
      source !== "wordbook" ||
      wordbookId === null ||
      pendingMistakeRef.current ||
      completingReview.current ||
      deletingRef.current
    )
      return;
    const question = state.questions[state.questionIndex];
    if (question.reviewId === undefined) return;
    setDeleteError(null);
    setDeleteTarget({
      roundKey,
      questionIndex: state.questionIndex,
      reviewId: question.reviewId,
      wordbookId,
      entry: question.entry,
    });
  };
  const confirmDelete = async () => {
    const target = deleteTarget;
    if (!target || deletingRef.current || state.phase !== "practice") return;
    if (
      roundKey !== target.roundKey ||
      state.questionIndex !== target.questionIndex ||
      state.questions[state.questionIndex].reviewId !== target.reviewId ||
      pendingMistakeRef.current ||
      completingReview.current
    ) {
      setDeleteTarget(null);
      return;
    }
    deletingRef.current = true;
    setIsDeleting(true);
    setDeleteError(null);
    try {
      const removed = await service.deleteWordbookEntry(
        target.wordbookId,
        target.entry.id,
      );
      if (!removed)
        throw new Error("该单词已不在当前单词本中，请重新开始练习。");
      const currentElapsed = elapsed(startedAt, now, elapsedSeconds);
      setElapsedSeconds(currentElapsed);
      setWordbooks(
        (listed) =>
          listed?.map((book) =>
            book.id === target.wordbookId
              ? { ...book, entryCount: Math.max(0, book.entryCount - 1) }
              : book,
          ) ?? null,
      );
      dispatch({
        type: "delete-question",
        reviewId: target.reviewId,
        elapsedSeconds: currentElapsed,
      });
      setDeleteTarget(null);
    } catch (error) {
      setDeleteError(
        `删除失败：${error instanceof Error ? error.message : String(error)}`,
      );
      setDeleteTarget(null);
    } finally {
      deletingRef.current = false;
      setIsDeleting(false);
    }
  };

  if (state.phase !== "practice") return null;
  return (
    <Stack gap={4}>
      {favoriteWriteError && !favoriteError ? (
        <Stack gap={2}>
          <Text role="alert">{favoriteWriteError}</Text>
          <Button
            label="关闭收藏错误提示"
            variant="ghost"
            onClick={() => setFavoriteWriteError(null)}
          />
        </Stack>
      ) : null}
      {mistakeWriteError ? (
        <Stack gap={2}>
          <Text role="alert">{mistakeWriteError}</Text>
          {pendingMistake ? (
            <Button
              label="重试保存错题"
              variant="secondary"
              isLoading={isSavingMistake}
              onClick={() => void saveMistake(pendingMistake)}
            />
          ) : (
            <Button
              label="关闭错题错误提示"
              variant="ghost"
              onClick={() => setMistakeWriteError(null)}
            />
          )}
        </Stack>
      ) : null}
      {isSavingMistake ? <Text role="status">正在保存错题记录…</Text> : null}
      {reviewWriteError ? <Text role="alert">{reviewWriteError}</Text> : null}
      {deleteError ? <Text role="alert">{deleteError}</Text> : null}
      {navigationNotice ? (
        <Text role="alert">请使用“退出并结算”保存当前练习后再离开。</Text>
      ) : null}
      <PracticeQuestion
        state={state}
        elapsedSeconds={elapsedSeconds}
        isCompleting={isCompleting}
        isBlocked={pendingMistake !== null || isDeleting}
        isFavorite={isFavorite}
        canDelete={source === "wordbook"}
        isDeleting={isDeleting}
        onRequestDelete={requestDelete}
        isFavoriteBusy={isFavoriteBusy}
        favoriteError={favoriteError}
        onToggleFavorite={() => void toggleFavorite()}
        onRetryFavorite={() => setFavoriteRefresh((value) => value + 1)}
        onChangeAnswer={(answer) => dispatch({ type: "change-answer", answer })}
        onHint={() => {
          if (
            completingReview.current ||
            pendingMistakeRef.current ||
            deletingRef.current
          )
            return;
          if (state.hintLevel === 0)
            dispatch({
              type: "start-hints",
              hints: createEnglishHints(
                getExpectedAnswer(state.questions[state.questionIndex]),
                random,
              ),
            });
          else dispatch({ type: "advance-hint" });
        }}
        onSkip={() => {
          if (
            !completingReview.current &&
            !pendingMistakeRef.current &&
            !deletingRef.current
          )
            dispatch({ type: "skip-question" });
        }}
        onSubmit={submitAnswer}
        onExit={() => void completeQuestion("exit-and-settle")}
        onContinue={() => void completeQuestion("continue-after-reveal")}
      />
      <AlertDialog
        isOpen={deleteTarget !== null}
        onOpenChange={(open) => {
          if (!open && !deletingRef.current) setDeleteTarget(null);
        }}
        title="从单词本删除这个单词？"
        description={
          deleteTarget
            ? `确认后将从当前单词本永久删除“${deleteTarget.entry.english} — ${deleteTarget.entry.chinese}”。`
            : "确认后将从当前单词本永久删除该单词。"
        }
        actionLabel="删除单词"
        cancelLabel="取消"
        isActionLoading={isDeleting}
        onAction={() => void confirmDelete()}
      />
    </Stack>
  );
}
export const Route = createFileRoute("/practice")({ component: PracticePage });

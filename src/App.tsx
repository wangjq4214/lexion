import { AlertDialog } from "@astryxdesign/core/AlertDialog";
import { AppShell } from "@astryxdesign/core/AppShell";
import { Button } from "@astryxdesign/core/Button";
import { Layout, LayoutContent } from "@astryxdesign/core/Layout";
import { Stack } from "@astryxdesign/core/Stack";
import { Text } from "@astryxdesign/core/Text";
import { useToast } from "@astryxdesign/core/Toast";
import * as stylex from "@stylexjs/stylex";
import {
  type ReactNode,
  useCallback,
  useEffect,
  useLayoutEffect,
  useReducer,
  useRef,
  useState,
} from "react";
import {
  type MistakeEntry,
  tauriWordbookService,
  type WordbookService,
  type WordbookSummary,
} from "./data/wordbooks";
import {
  createEnglishHints,
  getExpectedAnswer,
  isExactAnswer,
  type PracticeMode,
  type PracticeSource,
  type RandomSource,
} from "./domain/practice";
import type { WordEntry } from "./domain/word";
import { FavoritesList } from "./features/favorites/FavoritesList";
import { MistakesList } from "./features/mistakes/MistakesList";
import { PracticeQuestion } from "./features/practice/PracticeQuestion";
import { PracticeSetup } from "./features/practice/PracticeSetup";
import { PracticeSummary } from "./features/practice/PracticeSummary";
import { appReducer, initialState } from "./features/practice/practiceReducer";
import { WordbookImportFlow } from "./features/wordbooks/WordbookImportFlow";
import { tokens } from "./styles/tokens.stylex";

const styles = stylex.create({
  documentRoot: {
    MozOsxFontSmoothing: "grayscale",
    WebkitFontSmoothing: "antialiased",
    fontSynthesis: "none",
    backgroundColor: tokens.appBackground,
    color: tokens.appText,
    fontFamily: "var(--font-family-body)",
    fontSize: "var(--text-body-size)",
    fontWeight: "var(--text-body-weight)",
    lineHeight: "var(--text-body-leading)",
    textRendering: "optimizeLegibility",
    textSizeAdjust: "100%",
  },
});

function parsePracticeCount(value: string): number | null {
  if (!/^[0-9]+$/.test(value)) return null;
  const count = Number(value);
  return Number.isInteger(count) && count >= 1 && count <= 255 ? count : null;
}

type TimeSource = () => number;

const defaultNow: TimeSource = () => performance.now();

function getElapsedSeconds(startedAt: number, now: number) {
  return Math.max(0, Math.floor((now - startedAt) / 1_000));
}

function newSubmissionId(): string {
  const bytes = crypto.getRandomValues(new Uint8Array(16));
  return Array.from(bytes, (byte) => byte.toString(16).padStart(2, "0")).join(
    "",
  );
}

async function withWriteTimeout<T>(
  write: Promise<T>,
  message: string,
): Promise<T> {
  let timeout: ReturnType<typeof setTimeout> | undefined;
  try {
    return await Promise.race([
      write,
      new Promise<never>((_, reject) => {
        timeout = setTimeout(() => reject(new Error(message)), 10_000);
      }),
    ]);
  } finally {
    if (timeout !== undefined) clearTimeout(timeout);
  }
}

type MistakeSubmission = {
  id: string;
  english: string;
  chinese: string;
};

type AppProps = {
  random?: RandomSource;
  now?: TimeSource;
  wordbookService?: WordbookService;
};

function App({
  random = Math.random,
  now = defaultNow,
  wordbookService = tauriWordbookService,
}: AppProps) {
  const [state, dispatch] = useReducer(appReducer, initialState);
  const showToast = useToast();
  const [wordbooks, setWordbooks] = useState<WordbookSummary[] | null>(null);
  const [activeWordbookId, setActiveWordbookId] = useState<number | null>(null);
  const [loadError, setLoadError] = useState<string | null>(null);
  const [practiceSource, setPracticeSource] =
    useState<PracticeSource>("wordbook");
  const [showFavorites, setShowFavorites] = useState(false);
  const [favorites, setFavorites] = useState<WordEntry[] | null>(null);
  const [showMistakes, setShowMistakes] = useState(false);
  const [mistakes, setMistakes] = useState<MistakeEntry[] | null>(null);
  const [mistakesError, setMistakesError] = useState<string | null>(null);
  const [mistakeWriteError, setMistakeWriteError] = useState<string | null>(
    null,
  );
  const [reviewWriteError, setReviewWriteError] = useState<string | null>(null);
  const completingReview = useRef(false);
  const pendingMistakeRef = useRef<MistakeSubmission | null>(null);
  const savingMistakeRef = useRef(false);
  const [pendingMistake, setPendingMistake] =
    useState<MistakeSubmission | null>(null);
  const [isSavingMistake, setIsSavingMistake] = useState(false);
  const mistakesRequest = useRef(0);
  const [favoritesError, setFavoritesError] = useState<string | null>(null);
  const [favoriteWriteError, setFavoriteWriteError] = useState<string | null>(
    null,
  );
  const [removingFavoriteId, setRemovingFavoriteId] = useState<number | null>(
    null,
  );
  const [favoriteStatus, setFavoriteStatus] = useState<{
    key: string;
    saved: boolean;
  } | null>(null);
  const [favoriteErrorState, setFavoriteErrorState] = useState<{
    key: string;
    message: string;
  } | null>(null);
  const [favoriteRefresh, setFavoriteRefresh] = useState(0);
  const [isFavoriteBusy, setIsFavoriteBusy] = useState(false);
  const favoriteRequest = useRef(0);
  const favoritesRequest = useRef(0);
  const favoritesOpen = useRef(false);
  const favoriteBusy = useRef(false);
  const [practiceError, setPracticeError] = useState<string | null>(null);
  const [isStarting, setIsStarting] = useState(false);
  const [countSelection, setCountSelection] = useState("10");
  const [isCompleting, setIsCompleting] = useState(false);
  const [customCount, setCustomCount] = useState("");
  const practiceCount = parsePracticeCount(
    countSelection === "custom" ? customCount : countSelection,
  );
  const [elapsedSeconds, setElapsedSeconds] = useState(0);
  const roundStartedAt = useRef<number | null>(null);
  const [roundKey, setRoundKey] = useState(0);
  const [deleteTarget, setDeleteTarget] = useState<{
    roundKey: number;
    questionIndex: number;
    reviewId: number;
    wordbookId: number;
    entry: WordEntry;
  } | null>(null);
  const deletingRef = useRef(false);
  const [isDeleting, setIsDeleting] = useState(false);
  const [deleteError, setDeleteError] = useState<string | null>(null);

  useLayoutEffect(() => {
    const { className } = stylex.props(styles.documentRoot);
    const classNames = className?.split(" ") ?? [];
    document.documentElement.classList.add(...classNames);
    return () => document.documentElement.classList.remove(...classNames);
  }, []);

  const refreshWordbooks = useCallback(
    async (preferredId?: number) => {
      try {
        const listed = await wordbookService.listWordbooks();
        setWordbooks(listed);
        setLoadError(null);
        setActiveWordbookId((current) => {
          if (current !== null && listed.some(({ id }) => id === current)) {
            return current;
          }
          if (
            preferredId !== undefined &&
            listed.some(({ id }) => id === preferredId)
          ) {
            return preferredId;
          }
          return listed[0]?.id ?? null;
        });
      } catch (caught) {
        setLoadError(caught instanceof Error ? caught.message : String(caught));
        setWordbooks(null);
      }
    },
    [wordbookService],
  );

  useEffect(() => {
    let isCurrent = true;
    void (async () => {
      try {
        const listed = await wordbookService.listWordbooks();
        if (!isCurrent) return;
        setWordbooks(listed);
        setLoadError(null);
        setActiveWordbookId(listed[0]?.id ?? null);
      } catch (caught) {
        if (!isCurrent) return;
        setLoadError(caught instanceof Error ? caught.message : String(caught));
        setWordbooks(null);
      }
    })();
    return () => {
      isCurrent = false;
    };
  }, [wordbookService]);

  useEffect(() => {
    if (state.phase !== "practice") return;

    const intervalId = window.setInterval(() => {
      if (roundStartedAt.current === null) return;
      setElapsedSeconds(getElapsedSeconds(roundStartedAt.current, now()));
    }, 1_000);

    return () => window.clearInterval(intervalId);
  }, [state.phase, now]);
  const currentEntry =
    state.phase === "practice"
      ? state.questions[state.questionIndex].entry
      : null;
  const currentEnglish = currentEntry?.english;
  const currentChinese = currentEntry?.chinese;
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

  useEffect(() => {
    const request = ++favoriteRequest.current;
    if (
      favoriteQueryKey === null ||
      currentEnglish === undefined ||
      currentChinese === undefined
    )
      return;
    void wordbookService.isFavorite(currentEnglish, currentChinese).then(
      (saved) => {
        if (favoriteRequest.current === request) {
          setFavoriteStatus({ key: favoriteQueryKey, saved });
          setFavoriteErrorState(null);
        }
      },
      (error) => {
        if (favoriteRequest.current !== request) return;
        setFavoriteErrorState({
          key: favoriteQueryKey,
          message: error instanceof Error ? error.message : String(error),
        });
      },
    );
    return () => {
      favoriteRequest.current += 1;
    };
  }, [currentEnglish, currentChinese, favoriteQueryKey, wordbookService]);

  const toggleFavorite = async () => {
    if (!currentEntry || isFavorite === null || favoriteBusy.current) return;
    favoriteBusy.current = true;
    setIsFavoriteBusy(true);
    setFavoriteErrorState(null);
    setFavoriteWriteError(null);
    const request = favoriteRequest.current;
    try {
      if (isFavorite) {
        await wordbookService.removeFavorite(
          currentEntry.english,
          currentEntry.chinese,
        );
      } else {
        await wordbookService.addFavorite(
          currentEntry.english,
          currentEntry.chinese,
        );
      }
      if (favoriteRequest.current === request && favoriteQueryKey !== null) {
        setFavoriteStatus({ key: favoriteQueryKey, saved: !isFavorite });
      }
      showToast({
        body: isFavorite ? "已取消" : "已收藏",
        uniqueID: "favorite-feedback",
        autoHideDuration: 2500,
      });
      if (favoritesOpen.current) openFavorites();
    } catch (error) {
      const message = error instanceof Error ? error.message : String(error);
      if (favoriteRequest.current === request && favoriteQueryKey !== null) {
        setFavoriteErrorState({ key: favoriteQueryKey, message });
      } else {
        setFavoriteWriteError(`收藏操作失败：${message}`);
      }
    } finally {
      favoriteBusy.current = false;
      setIsFavoriteBusy(false);
    }
  };

  const openFavorites = () => {
    favoritesOpen.current = true;
    const request = ++favoritesRequest.current;
    setFavorites(null);
    setFavoritesError(null);
    setShowFavorites(true);
    void wordbookService.listFavorites().then(
      (entries) => {
        if (favoritesRequest.current === request) setFavorites(entries);
      },
      (error) => {
        if (favoritesRequest.current === request) {
          setFavoritesError(
            error instanceof Error ? error.message : String(error),
          );
        }
      },
    );
  };
  const openMistakes = () => {
    const request = ++mistakesRequest.current;
    setMistakes(null);
    setMistakesError(null);
    setShowMistakes(true);
    void (async () => {
      try {
        if (mistakesRequest.current !== request) return;
        const entries = await wordbookService.listMistakes();
        if (mistakesRequest.current === request) setMistakes(entries);
      } catch (error) {
        if (mistakesRequest.current === request) {
          setMistakesError(
            error instanceof Error ? error.message : String(error),
          );
        }
      }
    })();
  };

  const removeFavorite = async (entry: WordEntry) => {
    if (removingFavoriteId !== null) return;
    setRemovingFavoriteId(entry.id);
    setFavoritesError(null);
    try {
      await wordbookService.removeFavorite(entry.english, entry.chinese);
      if (favoritesOpen.current) openFavorites();
    } catch (error) {
      setFavoritesError(error instanceof Error ? error.message : String(error));
    } finally {
      setRemovingFavoriteId(null);
    }
  };

  const startPractice = async (mode: PracticeMode) => {
    if (activeWordbookId === null || isStarting || practiceCount === null)
      return;
    setIsStarting(true);
    setPracticeError(null);
    setReviewWriteError(null);
    setDeleteError(null);
    setDeleteTarget(null);
    try {
      const entries = await wordbookService.schedulePractice({
        source: practiceSource,
        wordbookId: practiceSource === "wordbook" ? activeWordbookId : null,
        limit: practiceCount,
        mode,
      });
      if (entries.length === 0) {
        setPracticeError(
          practiceSource === "favorites"
            ? "收藏夹还没有可练习的单词。"
            : practiceSource === "mistakes"
              ? "错题本还没有可练习的单词。"
              : "这个单词本没有可练习的词条。",
        );
        return;
      }
      setRoundKey((value) => value + 1);
      roundStartedAt.current = now();
      setElapsedSeconds(0);
      dispatch({
        type: "start",
        mode,
        questions: entries.map(({ reviewId, entry, direction }) => ({
          id: String(reviewId),
          reviewId,
          entry,
          direction,
        })),
      });
    } catch (caught) {
      setPracticeError(
        caught instanceof Error ? caught.message : String(caught),
      );
    } finally {
      setIsStarting(false);
    }
  };

  const requestDelete = () => {
    if (
      state.phase !== "practice" ||
      practiceSource !== "wordbook" ||
      activeWordbookId === null ||
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
      wordbookId: activeWordbookId,
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
      const removed = await wordbookService.deleteWordbookEntry(
        target.wordbookId,
        target.entry.id,
      );
      if (!removed)
        throw new Error("该单词已不在当前单词本中，请重新开始练习。");
      const currentElapsedSeconds =
        roundStartedAt.current === null
          ? elapsedSeconds
          : getElapsedSeconds(roundStartedAt.current, now());
      setElapsedSeconds(currentElapsedSeconds);
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
        elapsedSeconds: currentElapsedSeconds,
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

  const saveMistake = async (submission: MistakeSubmission) => {
    if (savingMistakeRef.current) return;
    savingMistakeRef.current = true;
    setIsSavingMistake(true);
    setMistakeWriteError(null);
    const write = wordbookService.recordMistakeOnce(
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
    const currentElapsedSeconds =
      roundStartedAt.current === null
        ? elapsedSeconds
        : getElapsedSeconds(roundStartedAt.current, now());
    if (type === "exit-and-settle" && state.revealReason === null) {
      setElapsedSeconds(currentElapsedSeconds);
      dispatch({ type, elapsedSeconds: currentElapsedSeconds });
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
        wordbookService.completeReview({
          reviewId: question.reviewId,
          errorCount: state.questionErrorCount,
          hintCount: state.questionHintCount,
          skipped: type !== "submit-answer" && state.revealReason === "skip",
        }),
        "复习进度尚未保存，请稍后重试。",
      );
      setElapsedSeconds(currentElapsedSeconds);
      dispatch({ type, elapsedSeconds: currentElapsedSeconds });
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
        id: newSubmissionId(),
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

  let content: ReactNode;

  if (loadError !== null) {
    content = (
      <Stack gap={4}>
        <Text role="alert">无法加载单词本：{loadError}</Text>
        <Button
          label="重试"
          variant="primary"
          onClick={() => refreshWordbooks()}
        />
      </Stack>
    );
  } else if (wordbooks === null) {
    content = <Text role="status">正在加载单词本…</Text>;
  } else if (wordbooks.length === 0) {
    content = (
      <WordbookImportFlow
        service={wordbookService}
        onImported={(result) => refreshWordbooks(result.wordbook.id)}
      />
    );
  } else if (showFavorites && state.phase === "setup") {
    content = (
      <FavoritesList
        entries={favorites}
        error={favoritesError}
        removingId={removingFavoriteId}
        onRemove={removeFavorite}
        onRetry={openFavorites}
        onBack={() => {
          favoritesRequest.current += 1;
          favoritesOpen.current = false;
          setShowFavorites(false);
        }}
      />
    );
  } else if (showMistakes && state.phase === "setup") {
    content = (
      <MistakesList
        entries={mistakes}
        error={mistakesError}
        onRetry={openMistakes}
        onBack={() => {
          mistakesRequest.current += 1;
          setShowMistakes(false);
        }}
      />
    );
  } else if (state.phase === "setup") {
    content = (
      <PracticeSetup
        mode={state.mode}
        wordbooks={wordbooks}
        wordbookService={wordbookService}
        activeWordbookId={activeWordbookId}
        onSelectWordbook={setActiveWordbookId}
        practiceSource={practiceSource}
        onSelectSource={(source) => {
          setPracticeSource(source);
          setPracticeError(null);
        }}
        onOpenFavorites={openFavorites}
        onOpenMistakes={openMistakes}
        onImported={refreshWordbooks}
        onSelectMode={(mode) => dispatch({ type: "select-mode", mode })}
        countSelection={countSelection}
        onCountSelectionChange={setCountSelection}
        customCount={customCount}
        onCustomCountChange={setCustomCount}
        practiceCount={practiceCount}
        practiceError={practiceError}
        isStarting={isStarting}
        onStart={() => startPractice(state.mode)}
      />
    );
  } else if (state.phase === "practice") {
    const question = state.questions[state.questionIndex];
    content = (
      <PracticeQuestion
        state={state}
        elapsedSeconds={elapsedSeconds}
        isCompleting={isCompleting}
        isBlocked={pendingMistake !== null || isDeleting}
        isFavorite={isFavorite}
        canDelete={practiceSource === "wordbook"}
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
          if (state.hintLevel === 0) {
            dispatch({
              type: "start-hints",
              hints: createEnglishHints(getExpectedAnswer(question), random),
            });
          } else {
            dispatch({ type: "advance-hint" });
          }
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
    );
  } else {
    content = (
      <PracticeSummary
        state={state}
        practiceError={practiceError}
        isStarting={isStarting}
        onBack={() => dispatch({ type: "back-to-setup" })}
        onRestart={() => startPractice(state.mode)}
      />
    );
  }

  return (
    <AppShell variant="surface" contentPadding={0} mobileNav={false}>
      <Layout
        contentWidth={640}
        height="fill"
        padding={6}
        content={
          <LayoutContent label="单词练习主内容">
            <Stack gap={4}>
              {favoriteWriteError ? (
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
              {isSavingMistake ? (
                <Text role="status">正在保存错题记录…</Text>
              ) : null}
              {reviewWriteError ? (
                <Text role="alert">{reviewWriteError}</Text>
              ) : null}
              {deleteError ? <Text role="alert">{deleteError}</Text> : null}
              {content}
            </Stack>
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
          </LayoutContent>
        }
      />
    </AppShell>
  );
}

export default App;

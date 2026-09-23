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
  tauriWordbookService,
  type WordbookService,
  type WordbookSummary,
} from "./data/wordbooks";
import {
  createEnglishHints,
  createQuestions,
  getExpectedAnswer,
  type PracticeMode,
  type RandomSource,
} from "./domain/practice";
import type { WordEntry } from "./domain/word";
import { FavoritesList } from "./features/favorites/FavoritesList";
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
  const [practiceSource, setPracticeSource] = useState<
    "wordbook" | "favorites"
  >("wordbook");
  const [showFavorites, setShowFavorites] = useState(false);
  const [favorites, setFavorites] = useState<WordEntry[] | null>(null);
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
  const [customCount, setCustomCount] = useState("");
  const practiceCount = parsePracticeCount(
    countSelection === "custom" ? customCount : countSelection,
  );
  const [elapsedSeconds, setElapsedSeconds] = useState(0);
  const roundStartedAt = useRef<number | null>(null);
  const [roundKey, setRoundKey] = useState(0);

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
    try {
      const entries =
        practiceSource === "favorites"
          ? await wordbookService.sampleFavorites(practiceCount)
          : await wordbookService.sampleWordbook(
              activeWordbookId,
              practiceCount,
            );
      if (entries.length === 0) {
        setPracticeError(
          practiceSource === "favorites"
            ? "收藏夹还没有可练习的单词。"
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
        questions: createQuestions(entries, mode, random),
      });
    } catch (caught) {
      setPracticeError(
        caught instanceof Error ? caught.message : String(caught),
      );
    } finally {
      setIsStarting(false);
    }
  };

  const completeQuestion = (type: "submit-answer" | "continue-after-skip") => {
    const currentElapsedSeconds =
      roundStartedAt.current === null
        ? elapsedSeconds
        : getElapsedSeconds(roundStartedAt.current, now());
    setElapsedSeconds(currentElapsedSeconds);
    dispatch({ type, elapsedSeconds: currentElapsedSeconds });
  };
  const submitAnswer = () => completeQuestion("submit-answer");

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
        isFavorite={isFavorite}
        isFavoriteBusy={isFavoriteBusy}
        favoriteError={favoriteError}
        onToggleFavorite={() => void toggleFavorite()}
        onRetryFavorite={() => setFavoriteRefresh((value) => value + 1)}
        onChangeAnswer={(answer) => dispatch({ type: "change-answer", answer })}
        onHint={() =>
          state.hintLevel === 0
            ? dispatch({
                type: "start-hints",
                hints: createEnglishHints(getExpectedAnswer(question), random),
              })
            : dispatch({ type: "advance-hint" })
        }
        onSkip={() => dispatch({ type: "skip-question" })}
        onSubmit={submitAnswer}
        onContinue={() => completeQuestion("continue-after-skip")}
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
              {content}
            </Stack>
          </LayoutContent>
        }
      />
    </AppShell>
  );
}

export default App;

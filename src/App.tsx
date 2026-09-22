import { AppShell } from "@astryxdesign/core/AppShell";
import { Button } from "@astryxdesign/core/Button";
import { Heading } from "@astryxdesign/core/Heading";
import { Layout, LayoutContent } from "@astryxdesign/core/Layout";
import { ProgressBar } from "@astryxdesign/core/ProgressBar";
import { Section } from "@astryxdesign/core/Section";
import { Selector } from "@astryxdesign/core/Selector";
import { Stack } from "@astryxdesign/core/Stack";
import { Text } from "@astryxdesign/core/Text";
import { TextInput } from "@astryxdesign/core/TextInput";
import {
  ToggleButton,
  ToggleButtonGroup,
} from "@astryxdesign/core/ToggleButton";
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
  createEnglishHint,
  createQuestions,
  getExpectedAnswer,
  getQuestionPrompt,
  isExactAnswer,
  type PracticeMode,
  type Question,
  type RandomSource,
} from "./domain/practice";
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

type SetupState = {
  phase: "setup";
  mode: PracticeMode;
};

type PracticeState = {
  phase: "practice";
  mode: PracticeMode;
  questions: Question[];
  questionIndex: number;
  answer: string;
  error: string | null;
  errorCount: number;
  hintCount: number;
  hint: string | null;
};

type SummaryState = {
  phase: "summary";
  mode: PracticeMode;
  correctCount: number;
  totalCount: number;
  errorCount: number;
  hintCount: number;
  elapsedSeconds: number;
};

type AppState = SetupState | PracticeState | SummaryState;

type AppAction =
  | { type: "select-mode"; mode: PracticeMode }
  | { type: "start"; mode: PracticeMode; questions: Question[] }
  | { type: "change-answer"; answer: string }
  | { type: "submit-answer"; elapsedSeconds: number }
  | { type: "reveal-hint"; hint: string }
  | { type: "back-to-setup" };

const initialState: AppState = {
  phase: "setup",
  mode: "zh-to-en",
};

function appReducer(state: AppState, action: AppAction): AppState {
  if (action.type === "select-mode") {
    return state.phase === "setup" ? { ...state, mode: action.mode } : state;
  }

  if (action.type === "start") {
    return {
      phase: "practice",
      mode: action.mode,
      questions: action.questions,
      questionIndex: 0,
      answer: "",
      error: null,
      errorCount: 0,
      hintCount: 0,
      hint: null,
    };
  }

  if (action.type === "back-to-setup") {
    return { phase: "setup", mode: state.mode };
  }

  if (state.phase !== "practice") {
    return state;
  }

  if (action.type === "change-answer") {
    return { ...state, answer: action.answer, error: null };
  }

  if (action.type === "reveal-hint") {
    if (state.hint !== null) return state;
    return { ...state, hint: action.hint, hintCount: state.hintCount + 1 };
  }

  if (action.type === "submit-answer") {
    const currentQuestion = state.questions[state.questionIndex];
    if (!isExactAnswer(state.answer, currentQuestion)) {
      return {
        ...state,
        error: "答案不完全匹配，请检查后重试。",
        errorCount: state.errorCount + 1,
      };
    }

    if (state.questionIndex === state.questions.length - 1) {
      return {
        phase: "summary",
        mode: state.mode,
        correctCount: state.questions.length,
        totalCount: state.questions.length,
        errorCount: state.errorCount,
        elapsedSeconds: action.elapsedSeconds,
        hintCount: state.hintCount,
      };
    }

    return {
      ...state,
      questionIndex: state.questionIndex + 1,
      answer: "",
      error: null,
      hint: null,
    };
  }

  return state;
}

const modeLabels: Record<PracticeMode, string> = {
  "zh-to-en": "看中文拼英文",
  "en-to-zh": "看英文拼中文",
  mixed: "双向混合随机",
};

type TimeSource = () => number;

const defaultNow: TimeSource = () => performance.now();

function getElapsedSeconds(startedAt: number, now: number) {
  return Math.max(0, Math.floor((now - startedAt) / 1_000));
}

function formatElapsedTime(elapsedSeconds: number) {
  const minutes = Math.floor(elapsedSeconds / 60);
  const seconds = elapsedSeconds % 60;
  return `${minutes}:${String(seconds).padStart(2, "0")}`;
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
  const [wordbooks, setWordbooks] = useState<WordbookSummary[] | null>(null);
  const [activeWordbookId, setActiveWordbookId] = useState<number | null>(null);
  const [loadError, setLoadError] = useState<string | null>(null);
  const [practiceError, setPracticeError] = useState<string | null>(null);
  const [isStarting, setIsStarting] = useState(false);
  const [elapsedSeconds, setElapsedSeconds] = useState(0);
  const roundStartedAt = useRef<number | null>(null);

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
  const startPractice = async (mode: PracticeMode) => {
    if (activeWordbookId === null || isStarting) return;
    setIsStarting(true);
    setPracticeError(null);
    try {
      const entries = await wordbookService.sampleWordbook(
        activeWordbookId,
        10,
      );
      if (entries.length === 0) {
        setPracticeError("这个单词本没有可练习的词条。");
        return;
      }
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

  const submitAnswer = () => {
    const currentElapsedSeconds =
      roundStartedAt.current === null
        ? elapsedSeconds
        : getElapsedSeconds(roundStartedAt.current, now());
    setElapsedSeconds(currentElapsedSeconds);
    dispatch({
      type: "submit-answer",
      elapsedSeconds: currentElapsedSeconds,
    });
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
  } else if (state.phase === "setup") {
    content = (
      <Stack gap={6}>
        <Stack direction="horizontal" gap={3} justify="between" wrap="wrap">
          <Stack gap={2}>
            <Heading level={1}>Lexicon 单词练习</Heading>
            <Text color="secondary">选择单词本和练习方式，开始本轮练习。</Text>
          </Stack>
          <WordbookImportFlow
            service={wordbookService}
            size="sm"
            variant="ghost"
            isDisabled={isStarting}
            onImported={(result) => refreshWordbooks(result.wordbook.id)}
          />
        </Stack>

        <Section>
          <Stack gap={4}>
            <Heading level={2}>练习设置</Heading>
            <Selector
              label="当前单词本"
              options={wordbooks.map((wordbook) => ({
                value: String(wordbook.id),
                label: wordbook.name,
                description: `${wordbook.entryCount} 个词条`,
              }))}
              value={
                activeWordbookId === null ? undefined : String(activeWordbookId)
              }
              isDisabled={isStarting}
              onChange={(value) => setActiveWordbookId(Number(value))}
              width="100%"
            />
            <ToggleButtonGroup
              label="练习模式"
              orientation="vertical"
              value={state.mode}
              isDisabled={isStarting}
              onChange={(value) => {
                if (value !== null) {
                  dispatch({
                    type: "select-mode",
                    mode: value as PracticeMode,
                  });
                }
              }}
            >
              <ToggleButton value="zh-to-en" label="看中文拼英文" />
              <ToggleButton value="en-to-zh" label="看英文拼中文" />
              <ToggleButton value="mixed" label="双向混合随机" />
            </ToggleButtonGroup>
            {practiceError ? <Text role="alert">{practiceError}</Text> : null}
            <Button
              label="开始练习"
              variant="primary"
              isLoading={isStarting}
              onClick={() => startPractice(state.mode)}
            />
          </Stack>
        </Section>
      </Stack>
    );
  } else if (state.phase === "practice") {
    const question = state.questions[state.questionIndex];
    const expectsEnglish = question.direction === "zh-to-en";
    content = (
      <Stack gap={6}>
        <Stack gap={2}>
          <Text color="secondary">{modeLabels[state.mode]}</Text>
          <ProgressBar
            label={`练习进度：第 ${state.questionIndex + 1} 题，共 ${state.questions.length} 题`}
            value={state.questionIndex}
            max={state.questions.length}
            hasValueLabel
            formatValueLabel={(value, max) => `${value} / ${max}`}
          />
          <Text color="secondary">
            本轮用时：{formatElapsedTime(elapsedSeconds)}
          </Text>
        </Stack>
        <Section>
          <Stack gap={6}>
            <Stack gap={2}>
              <Text color="secondary">
                {expectsEnglish ? "请拼写对应的英文" : "请输入对应的中文"}
              </Text>
              <Heading level={1}>{getQuestionPrompt(question)}</Heading>
            </Stack>
            <TextInput
              key={question.id}
              label={expectsEnglish ? "英文答案" : "中文答案"}
              value={state.answer}
              onChange={(answer) => dispatch({ type: "change-answer", answer })}
              onEnter={submitAnswer}
              placeholder={
                expectsEnglish ? "输入完整英文单词" : "输入完整中文释义"
              }
              status={
                state.error
                  ? { type: "error", message: state.error }
                  : undefined
              }
              statusVariant="detached"
              hasAutoFocus
              width="100%"
            />
            {state.hint ? (
              <Stack gap={1} role="status" aria-live="polite">
                <Text type="supporting">拼写提示</Text>
                <Text type="code" weight="semibold">
                  {state.hint}
                </Text>
              </Stack>
            ) : null}
            <Stack direction="horizontal" gap={3} wrap="wrap" justify="end">
              {expectsEnglish ? (
                <Button
                  label={state.hint ? "提示已显示" : "显示提示"}
                  variant="secondary"
                  isDisabled={state.hint !== null}
                  onClick={() =>
                    dispatch({
                      type: "reveal-hint",
                      hint: createEnglishHint(
                        getExpectedAnswer(question),
                        random,
                      ),
                    })
                  }
                />
              ) : null}
              <Button
                label="提交答案"
                variant="primary"
                onClick={submitAnswer}
              />
            </Stack>
          </Stack>
        </Section>
      </Stack>
    );
  } else {
    content = (
      <Stack gap={6}>
        <Stack gap={2} role="status" aria-live="polite">
          <Heading level={1}>本轮练习完成</Heading>
          <Text color="secondary">{modeLabels[state.mode]}</Text>
        </Stack>
        <Section>
          <Stack gap={4}>
            <Heading level={2}>练习结果</Heading>
            <Stack gap={2}>
              <Text>
                答对题数：{state.correctCount} / {state.totalCount}
              </Text>
              <Text>错误次数：{state.errorCount}</Text>
              <Text>提示次数：{state.hintCount}</Text>
              <Text>总用时：{formatElapsedTime(state.elapsedSeconds)}</Text>
            </Stack>
            {practiceError ? <Text role="alert">{practiceError}</Text> : null}
            <Stack direction="horizontal" gap={3} wrap="wrap" justify="end">
              <Button
                label="返回模式选择"
                variant="secondary"
                onClick={() => dispatch({ type: "back-to-setup" })}
              />
              <Button
                label="再练一轮"
                variant="primary"
                isLoading={isStarting}
                onClick={() => startPractice(state.mode)}
              />
            </Stack>
          </Stack>
        </Section>
      </Stack>
    );
  }

  return (
    <AppShell variant="surface" contentPadding={0} mobileNav={false}>
      <Layout
        contentWidth={640}
        height="fill"
        padding={6}
        content={
          <LayoutContent label="单词练习主内容">{content}</LayoutContent>
        }
      />
    </AppShell>
  );
}

export default App;

import { AppShell } from "@astryxdesign/core/AppShell";
import { Button } from "@astryxdesign/core/Button";
import { Heading } from "@astryxdesign/core/Heading";
import { Layout, LayoutContent } from "@astryxdesign/core/Layout";
import { ProgressBar } from "@astryxdesign/core/ProgressBar";
import { Section } from "@astryxdesign/core/Section";
import { Stack } from "@astryxdesign/core/Stack";
import { Text } from "@astryxdesign/core/Text";
import { TextInput } from "@astryxdesign/core/TextInput";
import {
  ToggleButton,
  ToggleButtonGroup,
} from "@astryxdesign/core/ToggleButton";
import * as stylex from "@stylexjs/stylex";
import { type ReactNode, useLayoutEffect, useReducer } from "react";
import { WORDS } from "./data/vocabulary";
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
  errorCount: number;
  hintCount: number;
};

type AppState = SetupState | PracticeState | SummaryState;

type AppAction =
  | { type: "select-mode"; mode: PracticeMode }
  | { type: "start"; mode: PracticeMode; questions: Question[] }
  | { type: "change-answer"; answer: string }
  | { type: "submit-answer" }
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
    if (state.hint !== null) {
      return state;
    }
    return {
      ...state,
      hint: action.hint,
      hintCount: state.hintCount + 1,
    };
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
        errorCount: state.errorCount,
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

type AppProps = {
  random?: RandomSource;
};

function App({ random = Math.random }: AppProps) {
  const [state, dispatch] = useReducer(appReducer, initialState);

  useLayoutEffect(() => {
    const { className } = stylex.props(styles.documentRoot);
    const classNames = className?.split(" ") ?? [];
    document.documentElement.classList.add(...classNames);

    return () => {
      document.documentElement.classList.remove(...classNames);
    };
  }, []);

  const startPractice = (mode: PracticeMode) => {
    dispatch({
      type: "start",
      mode,
      questions: createQuestions(WORDS, mode, random),
    });
  };

  let content: ReactNode;

  if (state.phase === "setup") {
    content = (
      <Stack gap={6}>
        <Stack gap={2}>
          <Heading level={1}>Lexicon 单词练习</Heading>
          <Text color="secondary">
            从 10 个固定词开始，选择一种中英文拼写练习方式。
          </Text>
        </Stack>

        <Section>
          <Stack gap={4}>
            <Heading level={2}>选择练习模式</Heading>
            <ToggleButtonGroup
              label="练习模式"
              orientation="vertical"
              value={state.mode}
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
            <Button
              label="开始练习"
              variant="primary"
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
              onEnter={() => dispatch({ type: "submit-answer" })}
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
                onClick={() => dispatch({ type: "submit-answer" })}
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
                答对题数：{state.correctCount} / {WORDS.length}
              </Text>
              <Text>错误次数：{state.errorCount}</Text>
              <Text>提示次数：{state.hintCount}</Text>
            </Stack>
            <Stack direction="horizontal" gap={3} wrap="wrap" justify="end">
              <Button
                label="返回模式选择"
                variant="secondary"
                onClick={() => dispatch({ type: "back-to-setup" })}
              />
              <Button
                label="再练一轮"
                variant="primary"
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

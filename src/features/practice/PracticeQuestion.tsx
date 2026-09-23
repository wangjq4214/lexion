import { Button } from "@astryxdesign/core/Button";
import { Heading } from "@astryxdesign/core/Heading";
import { ProgressBar } from "@astryxdesign/core/ProgressBar";
import { Section } from "@astryxdesign/core/Section";
import { Stack } from "@astryxdesign/core/Stack";
import { Text } from "@astryxdesign/core/Text";
import { TextInput } from "@astryxdesign/core/TextInput";
import { getQuestionPrompt } from "../../domain/practice";
import type { PracticeState } from "./practiceReducer";
import { formatElapsedTime, modeLabels } from "./presentation";

type PracticeQuestionProps = {
  state: PracticeState;
  elapsedSeconds: number;
  onChangeAnswer: (answer: string) => void;
  onHint: () => void;
  onSkip: () => void;
  onSubmit: () => void;
  onContinue: () => void;
};

export function PracticeQuestion({
  state,
  elapsedSeconds,
  onChangeAnswer,
  onHint,
  onSkip,
  onSubmit,
  onContinue,
}: PracticeQuestionProps) {
  const question = state.questions[state.questionIndex];
  const expectsEnglish = question.direction === "zh-to-en";
  return (
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
          {state.isAnswerRevealed ? (
            <Stack gap={2} role="status" aria-live="polite">
              <Text type="supporting">本题答案</Text>
              <Text>英文：{question.entry.english}</Text>
              <Text>中文释义：{question.entry.chinese}</Text>
              <Stack direction="horizontal" gap={3} justify="end">
                <Button
                  label={
                    state.questionIndex === state.questions.length - 1
                      ? "查看练习结果"
                      : "下一题"
                  }
                  variant="primary"
                  onClick={onContinue}
                />
              </Stack>
            </Stack>
          ) : (
            <>
              <TextInput
                key={question.id}
                label={expectsEnglish ? "英文答案" : "中文答案"}
                value={state.answer}
                onChange={onChangeAnswer}
                onEnter={onSubmit}
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
              {state.hintLevel > 0 && state.hints ? (
                <Stack gap={1} role="status" aria-live="polite">
                  <Text type="supporting">
                    拼写提示（第 {state.hintLevel} 级）
                  </Text>
                  <Text type="code" weight="semibold">
                    {state.hints[state.hintLevel - 1]}
                  </Text>
                </Stack>
              ) : null}
              <Stack direction="horizontal" gap={3} wrap="wrap" justify="end">
                {expectsEnglish ? (
                  <Button
                    label={
                      state.hintLevel === 2
                        ? "显示第 3 级提示并跳过"
                        : `显示第 ${state.hintLevel + 1} 级提示`
                    }
                    variant="secondary"
                    onClick={onHint}
                  />
                ) : null}
                <Button label="跳过" variant="secondary" onClick={onSkip} />
                <Button label="提交答案" variant="primary" onClick={onSubmit} />
              </Stack>
            </>
          )}
        </Stack>
      </Section>
    </Stack>
  );
}

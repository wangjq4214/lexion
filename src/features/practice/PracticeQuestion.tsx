import { Button } from "@astryxdesign/core/Button";
import { Heading } from "@astryxdesign/core/Heading";
import { IconButton } from "@astryxdesign/core/IconButton";
import { ProgressBar } from "@astryxdesign/core/ProgressBar";
import { Section } from "@astryxdesign/core/Section";
import { Stack } from "@astryxdesign/core/Stack";
import { Text } from "@astryxdesign/core/Text";
import { TextInput } from "@astryxdesign/core/TextInput";
import { ToggleButton } from "@astryxdesign/core/ToggleButton";
import { type AnswerSegment, diffAnswers } from "../../domain/answerDiff";
import { getExpectedAnswer, getQuestionPrompt } from "../../domain/practice";
import type { PracticeState } from "./practiceReducer";
import { formatElapsedTime, modeLabels } from "./presentation";

type PracticeQuestionProps = {
  state: PracticeState;
  elapsedSeconds: number;
  isCompleting: boolean;
  isBlocked: boolean;
  onChangeAnswer: (answer: string) => void;
  isFavorite: boolean | null;
  isFavoriteBusy: boolean;
  favoriteError: string | null;
  onToggleFavorite: () => void;
  onRetryFavorite: () => void;
  canDelete: boolean;
  isDeleting: boolean;
  onRequestDelete: () => void;
  onHint: () => void;
  onSkip: () => void;
  onSubmit: () => void;
  onContinue: () => void;
};

function renderDiff(parts: AnswerSegment[]) {
  if (parts.length === 0) return "（未填写）";
  let position = 0;
  return parts.map((part) => {
    const offset = position;
    position += part.text.length;
    return part.changed ? (
      <mark
        key={`${offset}:${part.text}`}
        style={{
          backgroundColor: "var(--color-error-muted)",
          color: "var(--color-text-primary)",
        }}
      >
        {part.text}
      </mark>
    ) : (
      part.text
    );
  });
}

function describeChanges(parts: AnswerSegment[]): string {
  let position = 0;
  const changes: string[] = [];
  for (const part of parts) {
    const characters = Array.from(part.text);
    if (part.changed) {
      const sample = characters.slice(0, 20).join("");
      const range =
        characters.length === 1
          ? `第 ${position + 1} 位`
          : `第 ${position + 1} 至 ${position + characters.length} 位`;
      changes.push(`${range}「${sample}${characters.length > 20 ? "…" : ""}」`);
    }
    position += characters.length;
  }
  return changes.length ? changes.join("、") : "无";
}
export function PracticeQuestion({
  state,
  elapsedSeconds,
  isCompleting,
  isBlocked,
  onChangeAnswer,
  isFavorite,
  isFavoriteBusy,
  favoriteError,
  onToggleFavorite,
  onRetryFavorite,
  canDelete,
  isDeleting,
  onRequestDelete,
  onHint,
  onSkip,
  onSubmit,
  onContinue,
}: PracticeQuestionProps) {
  const question = state.questions[state.questionIndex];
  const expectsEnglish = question.direction === "zh-to-en";
  const difference =
    state.revealReason === "wrong"
      ? diffAnswers(state.answer, getExpectedAnswer(question))
      : null;
  return (
    <Stack gap={6}>
      <Section paddingBlockStart={6} paddingBlockEnd={0}>
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
      </Section>
      <Section>
        <Stack gap={6}>
          <Stack gap={2}>
            <Text color="secondary">
              {expectsEnglish ? "请拼写对应的英文" : "请输入对应的中文"}
            </Text>
            <Stack direction="horizontal" gap={2} align="center">
              <Heading level={1}>{getQuestionPrompt(question)}</Heading>
              <ToggleButton
                label={isFavorite ? "取消收藏" : "收藏这个单词"}
                icon={<Text color="secondary">☆</Text>}
                pressedIcon={<Text color="accent">★</Text>}
                isPressed={isFavorite === true}
                size="sm"
                isIconOnly
                isLoading={isFavoriteBusy}
                isDisabled={isFavorite === null}
                onPressedChange={onToggleFavorite}
              />
              {canDelete ? (
                <IconButton
                  label="从单词本删除当前单词"
                  tooltip="从单词本删除当前单词"
                  icon={<Text color="secondary">🗑</Text>}
                  size="sm"
                  variant="ghost"
                  isLoading={isDeleting}
                  isDisabled={isCompleting || isBlocked}
                  onClick={onRequestDelete}
                />
              ) : null}
            </Stack>
          </Stack>
          {favoriteError ? <Text role="alert">{favoriteError}</Text> : null}
          {favoriteError && isFavorite === null ? (
            <Button
              label="重试收藏状态"
              variant="ghost"
              onClick={onRetryFavorite}
            />
          ) : null}
          {state.revealReason !== null ? (
            <Stack gap={2} role="status" aria-live="polite">
              {difference ? (
                <>
                  <Text type="supporting">本题答错，标记部分为答案差异</Text>
                  <Text>你的答案：{renderDiff(difference.submitted)}</Text>
                  <Text>正确答案：{renderDiff(difference.expected)}</Text>
                  <Text type="supporting">
                    差异说明：你的答案需核对{" "}
                    {describeChanges(difference.submitted)}； 正确答案需核对{" "}
                    {describeChanges(difference.expected)}。
                  </Text>
                </>
              ) : (
                <>
                  <Text type="supporting">本题答案</Text>
                  <Text>英文：{question.entry.english}</Text>
                  <Text>中文释义：{question.entry.chinese}</Text>
                </>
              )}
              <Stack direction="horizontal" gap={3} justify="end">
                <Button
                  label={
                    state.questionIndex === state.questions.length - 1
                      ? "查看练习结果"
                      : "下一题"
                  }
                  variant="primary"
                  isDisabled={isBlocked}
                  isLoading={isCompleting}
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
                isDisabled={isCompleting || isBlocked}
                onEnter={onSubmit}
                placeholder={
                  expectsEnglish ? "输入完整英文单词" : "输入完整中文释义"
                }
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
                    isDisabled={isCompleting || isBlocked}
                  />
                ) : null}
                <Button
                  label="跳过"
                  variant="secondary"
                  isDisabled={isCompleting || isBlocked}
                  onClick={onSkip}
                />
                <Button
                  label="提交答案"
                  variant="primary"
                  isLoading={isCompleting}
                  isDisabled={isBlocked}
                  onClick={onSubmit}
                />
              </Stack>
            </>
          )}
        </Stack>
      </Section>
    </Stack>
  );
}

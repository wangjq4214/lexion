import { Button } from "@astryxdesign/core/Button";
import { Heading } from "@astryxdesign/core/Heading";
import { Section } from "@astryxdesign/core/Section";
import { Stack } from "@astryxdesign/core/Stack";
import { Text } from "@astryxdesign/core/Text";
import type { SummaryState } from "./practiceReducer";
import { formatElapsedTime, modeLabels } from "./presentation";

type PracticeSummaryProps = {
  state: SummaryState;
  practiceError: string | null;
  isStarting: boolean;
  onBack: () => void;
  onRestart: () => void;
};

export function PracticeSummary({
  state,
  practiceError,
  isStarting,
  onBack,
  onRestart,
}: PracticeSummaryProps) {
  return (
    <Stack gap={6}>
      <Section paddingBlockStart={6} paddingBlockEnd={0}>
        <Stack gap={2} role="status" aria-live="polite">
          <Heading level={1}>本轮练习完成</Heading>
          <Text color="secondary">{modeLabels[state.mode]}</Text>
        </Stack>
      </Section>
      <Section>
        <Stack gap={4}>
          <Heading level={2}>练习结果</Heading>
          <Stack gap={2}>
            <Text>
              答对题数：{state.correctCount} / {state.totalCount}
            </Text>
            <Text>错误次数：{state.errorCount}</Text>
            <Text>提示次数：{state.hintCount}</Text>
            <Text>跳过次数：{state.skippedCount}</Text>
            <Text>总用时：{formatElapsedTime(state.elapsedSeconds)}</Text>
          </Stack>
          {practiceError ? <Text role="alert">{practiceError}</Text> : null}
          <Stack direction="horizontal" gap={3} wrap="wrap" justify="end">
            <Button label="返回模式选择" variant="secondary" onClick={onBack} />
            <Button
              label="再练一轮"
              variant="primary"
              isLoading={isStarting}
              onClick={onRestart}
            />
          </Stack>
        </Stack>
      </Section>
    </Stack>
  );
}

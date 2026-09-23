import { Button } from "@astryxdesign/core/Button";
import { Heading } from "@astryxdesign/core/Heading";
import { List, ListItem } from "@astryxdesign/core/List";
import { Section } from "@astryxdesign/core/Section";
import { Stack } from "@astryxdesign/core/Stack";
import { Text } from "@astryxdesign/core/Text";
import type { MistakeEntry } from "../../data/wordbooks";

type MistakesListProps = {
  entries: MistakeEntry[] | null;
  error: string | null;
  onRetry: () => void;
  onBack: () => void;
};

export function MistakesList({
  entries,
  error,
  onRetry,
  onBack,
}: MistakesListProps) {
  return (
    <Stack gap={6}>
      <Section paddingBlockStart={6} paddingBlockEnd={0}>
        <Stack direction="horizontal" gap={3} justify="between" wrap="wrap">
          <Heading level={1}>错题本</Heading>
          <Button label="返回练习设置" variant="ghost" onClick={onBack} />
        </Stack>
      </Section>
      <Section>
        <Stack gap={4}>
          {error ? (
            <Stack gap={2}>
              <Text role="alert">{error}</Text>
              <Button label="重试加载错题本" onClick={onRetry} />
            </Stack>
          ) : null}
          {entries === null ? (
            error ? null : (
              <Text role="status">正在加载错题本…</Text>
            )
          ) : entries.length === 0 ? (
            <Text>错题本还没有单词。提交错误答案后会记录在这里。</Text>
          ) : (
            <List header={<Heading level={2}>错误单词</Heading>} hasDividers>
              {entries.map((entry) => (
                <ListItem
                  key={entry.id}
                  label={entry.english}
                  description={entry.chinese}
                  endContent={<Text>错误次数：{entry.errorCount}</Text>}
                />
              ))}
            </List>
          )}
        </Stack>
      </Section>
    </Stack>
  );
}

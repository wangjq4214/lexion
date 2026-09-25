import { Button } from "@astryxdesign/core/Button";
import { Heading } from "@astryxdesign/core/Heading";
import { List, ListItem } from "@astryxdesign/core/List";
import { Section } from "@astryxdesign/core/Section";
import { Stack } from "@astryxdesign/core/Stack";
import { Text } from "@astryxdesign/core/Text";
import type { WordEntry } from "../../domain/word";

type FavoritesListProps = {
  entries: WordEntry[] | null;
  error: string | null;
  removingId: number | null;
  onRemove: (entry: WordEntry) => void;
  onRetry: () => void;
  backLabel: string;
  onBack: () => void;
};

export function FavoritesList({
  entries,
  error,
  removingId,
  onRemove,
  onRetry,
  backLabel,
  onBack,
}: FavoritesListProps) {
  return (
    <Stack gap={6}>
      <Section paddingBlockStart={6} paddingBlockEnd={0}>
        <Stack direction="horizontal" gap={3} justify="between" wrap="wrap">
          <Heading level={1}>收藏夹</Heading>
          <Button label={backLabel} variant="ghost" onClick={onBack} />
        </Stack>
      </Section>
      <Section>
        <Stack gap={4}>
          {error ? (
            <Stack gap={2}>
              <Text role="alert">{error}</Text>
              {entries === null ? (
                <Button label="重试加载收藏夹" onClick={onRetry} />
              ) : null}
            </Stack>
          ) : null}
          {entries === null ? (
            error ? null : (
              <Text role="status">正在加载收藏夹…</Text>
            )
          ) : entries.length === 0 ? (
            <Text>收藏夹还没有单词。练习时可以收藏当前单词。</Text>
          ) : (
            <List header={<Heading level={2}>已收藏单词</Heading>} hasDividers>
              {entries.map((entry) => (
                <ListItem
                  key={entry.id}
                  label={entry.english}
                  description={entry.chinese}
                  endContent={
                    <Button
                      label={`移除 ${entry.english}：${entry.chinese}`}
                      size="sm"
                      variant="ghost"
                      isLoading={removingId === entry.id}
                      isDisabled={removingId !== null}
                      onClick={() => onRemove(entry)}
                    />
                  }
                />
              ))}
            </List>
          )}
        </Stack>
      </Section>
    </Stack>
  );
}

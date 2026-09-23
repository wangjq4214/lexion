import { Button } from "@astryxdesign/core/Button";
import { Heading } from "@astryxdesign/core/Heading";
import { Section } from "@astryxdesign/core/Section";
import { Selector } from "@astryxdesign/core/Selector";
import { Stack } from "@astryxdesign/core/Stack";
import { Text } from "@astryxdesign/core/Text";
import { TextInput } from "@astryxdesign/core/TextInput";
import {
  ToggleButton,
  ToggleButtonGroup,
} from "@astryxdesign/core/ToggleButton";
import type { WordbookService, WordbookSummary } from "../../data/wordbooks";
import type { PracticeMode } from "../../domain/practice";
import { WordbookImportFlow } from "../wordbooks/WordbookImportFlow";

const practiceCountOptions = [
  ...[5, 10, 20, 50].map((count) => ({
    value: String(count),
    label: `${count} 个单词`,
  })),
  { value: "custom", label: "自定义" },
];

type PracticeSetupProps = {
  mode: PracticeMode;
  wordbooks: WordbookSummary[];
  wordbookService: WordbookService;
  activeWordbookId: number | null;
  onSelectWordbook: (id: number) => void;
  practiceSource: "wordbook" | "favorites";
  onSelectSource: (source: "wordbook" | "favorites") => void;
  onOpenFavorites: () => void;
  onImported: (id: number) => Promise<void>;
  onSelectMode: (mode: PracticeMode) => void;
  countSelection: string;
  onCountSelectionChange: (value: string) => void;
  customCount: string;
  onCustomCountChange: (value: string) => void;
  practiceCount: number | null;
  practiceError: string | null;
  isStarting: boolean;
  onStart: () => void;
};

export function PracticeSetup({
  mode,
  wordbooks,
  wordbookService,
  activeWordbookId,
  onSelectWordbook,
  practiceSource,
  onSelectSource,
  onOpenFavorites,
  onImported,
  onSelectMode,
  countSelection,
  onCountSelectionChange,
  customCount,
  onCustomCountChange,
  practiceCount,
  practiceError,
  isStarting,
  onStart,
}: PracticeSetupProps) {
  return (
    <Stack gap={6}>
      <Section paddingBlockStart={6} paddingBlockEnd={0}>
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
            onImported={(result) => onImported(result.wordbook.id)}
          />
        </Stack>
      </Section>

      <Section>
        <Stack gap={4}>
          <Heading level={2}>练习设置</Heading>
          <Selector
            label="练习来源"
            options={[
              { value: "wordbook", label: "单词本" },
              { value: "favorites", label: "收藏夹" },
            ]}
            value={practiceSource}
            isDisabled={isStarting}
            onChange={(value) =>
              onSelectSource(value as "wordbook" | "favorites")
            }
            width="100%"
          />
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
            isDisabled={isStarting || practiceSource === "favorites"}
            onChange={(value) => onSelectWordbook(Number(value))}
            width="100%"
          />
          <Button
            label="查看收藏夹"
            variant="secondary"
            isDisabled={isStarting}
            onClick={onOpenFavorites}
          />
          <ToggleButtonGroup
            label="练习模式"
            orientation="vertical"
            value={mode}
            isDisabled={isStarting}
            onChange={(value) => {
              if (value !== null) onSelectMode(value as PracticeMode);
            }}
          >
            <ToggleButton value="zh-to-en" label="看中文拼英文" />
            <ToggleButton value="en-to-zh" label="看英文拼中文" />
            <ToggleButton value="mixed" label="双向混合随机" />
          </ToggleButtonGroup>
          <Selector
            label="本轮单词数"
            options={practiceCountOptions}
            value={countSelection}
            isDisabled={isStarting}
            onChange={onCountSelectionChange}
            width="100%"
          />
          {countSelection === "custom" ? (
            <TextInput
              label="自定义单词数"
              description="请输入 1–255 的整数"
              value={customCount}
              onChange={onCustomCountChange}
              isDisabled={isStarting}
              status={
                practiceCount === null
                  ? { type: "error", message: "请输入 1–255 的整数。" }
                  : undefined
              }
              statusVariant="detached"
              width="100%"
            />
          ) : null}
          {practiceError ? <Text role="alert">{practiceError}</Text> : null}
          <Button
            label="开始练习"
            variant="primary"
            isLoading={isStarting}
            isDisabled={practiceCount === null}
            onClick={onStart}
          />
        </Stack>
      </Section>
    </Stack>
  );
}

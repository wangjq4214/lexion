import { AlertDialog } from "@astryxdesign/core/AlertDialog";
import { Button } from "@astryxdesign/core/Button";
import { Heading } from "@astryxdesign/core/Heading";
import { Section } from "@astryxdesign/core/Section";
import { Selector } from "@astryxdesign/core/Selector";
import { Stack } from "@astryxdesign/core/Stack";
import { Text } from "@astryxdesign/core/Text";
import { TextInput } from "@astryxdesign/core/TextInput";
import { useState } from "react";
import type { WordbookService, WordbookSummary } from "../../data/wordbooks";
import type { PracticeMode, PracticeSource } from "../../domain/practice";
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
  practiceSource: PracticeSource;
  onSelectSource: (source: PracticeSource) => void;
  onOpenFavorites: () => void;
  onOpenMistakes: () => void;
  onImported: (id: number) => Promise<void>;
  onDeleted: () => Promise<void>;
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
  onOpenMistakes,
  onImported,
  onDeleted,
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
  const [deleteTarget, setDeleteTarget] = useState<WordbookSummary | null>(
    null,
  );
  const [isDeleting, setIsDeleting] = useState(false);
  const [deleteError, setDeleteError] = useState<string | null>(null);
  const selectedWordbook = wordbooks.find(({ id }) => id === activeWordbookId);

  const confirmDelete = async () => {
    if (!deleteTarget || isDeleting) return;
    setIsDeleting(true);
    setDeleteError(null);
    try {
      const deleted = await wordbookService.deleteWordbook(deleteTarget.id);
      if (!deleted) throw new Error("单词本已不存在，请刷新后重试。");
      await onDeleted();
      setDeleteTarget(null);
    } catch (error) {
      setDeleteError(error instanceof Error ? error.message : String(error));
    } finally {
      setIsDeleting(false);
    }
  };

  return (
    <Stack gap={6}>
      <Section paddingBlockStart={6} paddingBlockEnd={0}>
        <Stack direction="horizontal" gap={3} justify="between" wrap="wrap">
          <Stack gap={2}>
            <Heading level={1}>Lexicon 单词练习</Heading>
            <Text color="secondary">选择练习内容和方式，开始本轮练习。</Text>
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
        <Stack gap={6}>
          <Heading level={2}>练习设置</Heading>
          <Stack gap={3}>
            <Heading level={3}>选择练习内容</Heading>
            <Selector
              label="练习来源"
              options={[
                {
                  value: "wordbook",
                  label: "单词本",
                  disabled: wordbooks.length === 0,
                },
                { value: "favorites", label: "收藏夹" },
                { value: "mistakes", label: "错题本" },
              ]}
              value={practiceSource}
              isDisabled={isStarting}
              onChange={(value) => onSelectSource(value as PracticeSource)}
              width="100%"
            />
            {practiceSource === "wordbook" ? (
              <Stack gap={2}>
                {wordbooks.length === 0 ? (
                  <Text color="secondary">
                    请先导入单词本，或选择收藏夹、错题本。
                  </Text>
                ) : (
                  <>
                    <Selector
                      label="当前单词本"
                      options={wordbooks.map((wordbook) => ({
                        value: String(wordbook.id),
                        label: wordbook.name,
                        description: `${wordbook.entryCount} 个词条`,
                      }))}
                      value={
                        activeWordbookId === null
                          ? undefined
                          : String(activeWordbookId)
                      }
                      isDisabled={isStarting || isDeleting}
                      onChange={(value) => onSelectWordbook(Number(value))}
                      width="100%"
                    />
                    {selectedWordbook ? (
                      <Button
                        label="删除当前单词本"
                        variant="ghost"
                        size="sm"
                        isDisabled={isStarting || isDeleting}
                        onClick={() => {
                          setDeleteError(null);
                          setDeleteTarget(selectedWordbook);
                        }}
                      />
                    ) : null}
                    {deleteError ? (
                      <Text role="alert">{deleteError}</Text>
                    ) : null}
                  </>
                )}
              </Stack>
            ) : null}
          </Stack>
          <Stack gap={3}>
            <Heading level={3}>设置练习方式</Heading>
            <Selector
              label="练习模式"
              options={[
                { value: "zh-to-en", label: "看中文拼英文" },
                { value: "en-to-zh", label: "看英文拼中文" },
                { value: "mixed", label: "双向混合随机" },
              ]}
              value={mode}
              isDisabled={isStarting}
              onChange={(value) => onSelectMode(value as PracticeMode)}
              width="100%"
            />
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
          </Stack>
          <Stack gap={2}>
            <Heading level={3}>浏览词库</Heading>
            <Stack direction="horizontal" gap={2} wrap="wrap" align="start">
              <Button
                label="查看收藏夹"
                variant="ghost"
                size="sm"
                isDisabled={isStarting}
                onClick={onOpenFavorites}
              />
              <Button
                label="查看错题本"
                variant="ghost"
                size="sm"
                isDisabled={isStarting}
                onClick={onOpenMistakes}
              />
            </Stack>
          </Stack>
          <Stack gap={3}>
            {practiceError ? <Text role="alert">{practiceError}</Text> : null}
            <Button
              label="开始练习"
              variant="primary"
              isLoading={isStarting}
              isDisabled={
                practiceCount === null ||
                (practiceSource === "wordbook" && !selectedWordbook)
              }
              onClick={onStart}
            />
          </Stack>
        </Stack>
      </Section>
      <AlertDialog
        isOpen={deleteTarget !== null}
        onOpenChange={(open) => {
          if (!open && !isDeleting) setDeleteTarget(null);
        }}
        title="删除单词本？"
        description={`确认后将永久删除“${deleteTarget?.name ?? ""}”及其中的词条。收藏夹和错题本不受影响。${deleteError ? `删除失败：${deleteError}` : ""}`}
        actionLabel="删除单词本"
        cancelLabel="取消"
        isActionLoading={isDeleting}
        onAction={() => void confirmDelete()}
      />
    </Stack>
  );
}

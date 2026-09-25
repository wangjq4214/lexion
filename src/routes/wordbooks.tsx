import { AlertDialog } from "@astryxdesign/core/AlertDialog";
import { Button } from "@astryxdesign/core/Button";
import { Heading } from "@astryxdesign/core/Heading";
import { List, ListItem } from "@astryxdesign/core/List";
import { Section } from "@astryxdesign/core/Section";
import { Stack } from "@astryxdesign/core/Stack";
import { Text } from "@astryxdesign/core/Text";
import { createFileRoute, useNavigate } from "@tanstack/react-router";
import { useAtomValue } from "jotai";
import { useEffect, useRef, useState } from "react";
import type { WordbookService, WordbookSummary } from "../data/wordbooks";
import type { WordEntry } from "../domain/word";
import { WordbookImportFlow } from "../features/wordbooks/WordbookImportFlow";
import { useRefreshWordbooks } from "../features/wordbooks/WordbookProvider";
import { wordbooksAtom } from "../state/appState";

function WordbooksPage() {
  const { props } = Route.useRouteContext();
  const service: WordbookService = props.wordbookService;
  const navigate = useNavigate();
  const refreshWordbooks = useRefreshWordbooks();
  const wordbooks = useAtomValue(wordbooksAtom) ?? [];
  const [selectedId, setSelectedId] = useState<number | null>(null);
  const selected =
    wordbooks.find(({ id }) => id === selectedId) ?? wordbooks[0];
  const selectedBookId = selected?.id;
  const [revision, setRevision] = useState(0);
  const [entryResult, setEntryResult] = useState<{
    bookId: number;
    revision: number;
    items: WordEntry[];
  } | null>(null);
  const entries =
    entryResult !== null &&
    entryResult.bookId === selectedBookId &&
    entryResult.revision === revision
      ? entryResult.items
      : null;
  const [loadFailure, setLoadFailure] = useState<{
    bookId: number;
    revision: number;
    message: string;
  } | null>(null);
  const loadError =
    loadFailure !== null &&
    loadFailure.bookId === selectedBookId &&
    loadFailure.revision === revision
      ? loadFailure.message
      : null;
  const [deleteBook, setDeleteBook] = useState<WordbookSummary | null>(null);
  const [deleteEntry, setDeleteEntry] = useState<{
    bookId: number;
    bookName: string;
    entry: WordEntry;
  } | null>(null);
  const [isDeleting, setIsDeleting] = useState(false);
  const deletingRef = useRef(false);
  const [deleteError, setDeleteError] = useState<string | null>(null);
  const [importedName, setImportedName] = useState<string | null>(null);

  useEffect(() => {
    if (selectedBookId === undefined) return;
    let current = true;
    void service.listWordbookEntries(selectedBookId).then(
      (items) => {
        if (current) {
          setLoadFailure(null);
          setEntryResult({ bookId: selectedBookId, revision, items });
        }
      },
      (error) => {
        if (current)
          setLoadFailure({
            bookId: selectedBookId,
            revision,
            message: error instanceof Error ? error.message : String(error),
          });
      },
    );
    return () => {
      current = false;
    };
  }, [service, selectedBookId, revision]);

  const removeBook = async () => {
    const target = deleteBook;
    if (!target || deletingRef.current) return;
    deletingRef.current = true;
    setIsDeleting(true);
    setDeleteError(null);
    try {
      if (!(await service.deleteWordbook(target.id))) {
        setDeleteBook(null);
        throw new Error("单词本已不存在，请刷新后重试。");
      }
      setDeleteBook(null);
      setSelectedId(null);
      await refreshWordbooks();
      setRevision((value) => value + 1);
    } catch (error) {
      setDeleteError(error instanceof Error ? error.message : String(error));
      await refreshWordbooks();
      setRevision((value) => value + 1);
    } finally {
      setIsDeleting(false);
      deletingRef.current = false;
    }
  };

  const removeEntry = async () => {
    const target = deleteEntry;
    if (!target || deletingRef.current) return;
    deletingRef.current = true;
    setIsDeleting(true);
    setDeleteError(null);
    try {
      if (
        !(await service.deleteWordbookEntry(target.bookId, target.entry.id))
      ) {
        setDeleteEntry(null);
        throw new Error("该单词已不在当前单词本中，请刷新后重试。");
      }
      setDeleteEntry(null);
      await refreshWordbooks();
      setRevision((value) => value + 1);
    } catch (error) {
      setDeleteError(error instanceof Error ? error.message : String(error));
      await refreshWordbooks();
      setRevision((value) => value + 1);
    } finally {
      setIsDeleting(false);
      deletingRef.current = false;
    }
  };

  return (
    <Stack gap={6}>
      <Section paddingBlockStart={6} paddingBlockEnd={0}>
        <Stack direction="horizontal" gap={3} justify="between" wrap="wrap">
          <Heading level={1}>单词本管理</Heading>
          <Button
            label="返回首页"
            variant="ghost"
            onClick={() => void navigate({ to: "/" })}
          />
        </Stack>
      </Section>
      {deleteError && !deleteBook && !deleteEntry ? (
        <Text role="alert">{deleteError}</Text>
      ) : null}
      <Section>
        <Stack gap={4}>
          <Stack direction="horizontal" gap={3} justify="between" wrap="wrap">
            <Heading level={2}>我的单词本</Heading>
            <WordbookImportFlow
              service={service}
              onImported={async ({ wordbook }) => {
                await refreshWordbooks(wordbook.id);
                setSelectedId(wordbook.id);
                setDeleteError(null);
                setRevision((value) => value + 1);
                setImportedName(wordbook.name);
              }}
            />
          </Stack>
          {importedName ? (
            <Text role="status">已导入“{importedName}”。</Text>
          ) : null}
          {wordbooks.length === 0 ? (
            <Text>还没有单词本，请导入 Excel 文件。</Text>
          ) : (
            <List header={<Heading level={3}>单词本列表</Heading>} hasDividers>
              {wordbooks.map((book) => (
                <ListItem
                  key={book.id}
                  label={book.name}
                  description={`${book.entryCount} 个词条`}
                  endContent={
                    <Button
                      label={`浏览 ${book.name}`}
                      variant={book.id === selected?.id ? "secondary" : "ghost"}
                      size="sm"
                      isDisabled={isDeleting}
                      onClick={() => {
                        setSelectedId(book.id);
                        setRevision((value) => value + 1);
                        setDeleteError(null);
                        setImportedName(null);
                      }}
                    />
                  }
                />
              ))}
            </List>
          )}
        </Stack>
      </Section>
      {selected ? (
        <Section>
          <Stack gap={4}>
            <Stack direction="horizontal" gap={3} justify="between" wrap="wrap">
              <Heading level={2}>{selected.name}中的单词</Heading>
              <Button
                label={`删除单词本 ${selected.name}`}
                variant="destructive"
                size="sm"
                isDisabled={isDeleting}
                onClick={() => {
                  setDeleteError(null);
                  setDeleteBook(selected);
                }}
              />
            </Stack>
            {loadError ? (
              <Stack gap={2}>
                <Text role="alert">加载词条失败：{loadError}</Text>
                <Button
                  label="重试加载词条"
                  onClick={() => setRevision((n) => n + 1)}
                />
              </Stack>
            ) : entries === null ? (
              <Text role="status">正在加载词条…</Text>
            ) : entries.length === 0 ? (
              <Text>这个单词本还没有单词。</Text>
            ) : (
              <List header={<Heading level={3}>词条列表</Heading>} hasDividers>
                {entries.map((entry) => (
                  <ListItem
                    key={entry.id}
                    label={entry.english}
                    description={entry.chinese}
                    endContent={
                      <Button
                        label={`删除 ${entry.english}：${entry.chinese}`}
                        size="sm"
                        variant="ghost"
                        isDisabled={isDeleting}
                        onClick={() => {
                          setDeleteError(null);
                          setDeleteEntry({
                            bookId: selected.id,
                            bookName: selected.name,
                            entry,
                          });
                        }}
                      />
                    }
                  />
                ))}
              </List>
            )}
          </Stack>
        </Section>
      ) : null}
      <AlertDialog
        isOpen={deleteBook !== null}
        onOpenChange={(open) => {
          if (!open && !isDeleting) setDeleteBook(null);
        }}
        title="删除单词本？"
        description={`确认后将永久删除“${deleteBook?.name ?? ""}”及其中的词条。收藏夹和错题本不受影响。${deleteBook && deleteError ? `删除失败：${deleteError}` : ""}`}
        actionLabel="删除单词本"
        cancelLabel="取消"
        isActionLoading={isDeleting}
        onAction={() => void removeBook()}
      />
      <AlertDialog
        isOpen={deleteEntry !== null}
        onOpenChange={(open) => {
          if (!open && !isDeleting) setDeleteEntry(null);
        }}
        title="从单词本删除单词？"
        description={`确认后将从“${deleteEntry?.bookName ?? ""}”删除“${deleteEntry?.entry.english ?? ""}：${deleteEntry?.entry.chinese ?? ""}”。收藏夹和错题本不受影响。${deleteEntry && deleteError ? `删除失败：${deleteError}` : ""}`}
        actionLabel="删除单词"
        cancelLabel="取消"
        isActionLoading={isDeleting}
        onAction={() => void removeEntry()}
      />
    </Stack>
  );
}

export const Route = createFileRoute("/wordbooks")({
  component: WordbooksPage,
});

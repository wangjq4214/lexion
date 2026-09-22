import { AlertDialog } from "@astryxdesign/core/AlertDialog";
import { Button } from "@astryxdesign/core/Button";
import { Dialog, DialogHeader } from "@astryxdesign/core/Dialog";
import { Layout, LayoutContent, LayoutFooter } from "@astryxdesign/core/Layout";
import { Stack } from "@astryxdesign/core/Stack";
import { Text } from "@astryxdesign/core/Text";
import { TextInput } from "@astryxdesign/core/TextInput";
import { useState } from "react";
import {
  defaultWordbookName,
  type ImportWordbookResult,
  WordbookError,
  type WordbookService,
} from "../../data/wordbooks";

type WordbookImportFlowProps = {
  service: WordbookService;
  onImported: (result: ImportWordbookResult) => Promise<void> | void;
  size?: "sm" | "md";
  isDisabled?: boolean;
  variant?: "primary" | "secondary" | "ghost";
};

export function WordbookImportFlow({
  service,
  onImported,
  size = "md",
  variant = "primary",
  isDisabled = false,
}: WordbookImportFlowProps) {
  const [path, setPath] = useState<string | null>(null);
  const [name, setName] = useState("");
  const [isPicking, setIsPicking] = useState(false);
  const [isImporting, setIsImporting] = useState(false);
  const [isConfirming, setIsConfirming] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const reset = () => {
    setPath(null);
    setName("");
    setError(null);
    setIsConfirming(false);
  };

  const chooseFile = async () => {
    if (isDisabled || isPicking || isImporting) return;
    setIsPicking(true);
    try {
      const selected = await service.pickWorkbookFile();
      if (selected !== null) {
        setPath(selected);
        setName(defaultWordbookName(selected));
        setError(null);
      }
    } catch (caught) {
      setError(caught instanceof Error ? caught.message : String(caught));
    } finally {
      setIsPicking(false);
    }
  };

  const importSelected = async (replaceExisting: boolean) => {
    if (path === null || isImporting) return;
    const trimmedName = name.trim();
    if (trimmedName.length === 0) {
      setError("单词本名称不能为空");
      return;
    }

    setIsImporting(true);
    setError(null);
    try {
      const result = await service.importWordbook({
        path,
        name: trimmedName,
        replaceExisting,
      });
      reset();
      await onImported(result);
    } catch (caught) {
      if (caught instanceof WordbookError && caught.kind === "conflict") {
        setIsConfirming(true);
      } else {
        setError(caught instanceof Error ? caught.message : String(caught));
      }
    } finally {
      setIsImporting(false);
    }
  };

  return (
    <>
      <Button
        label="导入单词本"
        variant={variant}
        size={size}
        isLoading={isPicking}
        isDisabled={isDisabled || isImporting}
        onClick={chooseFile}
      />
      {path === null && error ? <Text role="alert">{error}</Text> : null}
      <Dialog
        isOpen={path !== null && !isConfirming}
        onOpenChange={(isOpen) => {
          if (!isOpen && !isImporting) reset();
        }}
        purpose="form"
        width={480}
      >
        <Layout
          header={
            <DialogHeader
              title="导入单词本"
              subtitle="确认或修改从文件名生成的单词本名称"
              onOpenChange={(isOpen) => {
                if (!isOpen && !isImporting) reset();
              }}
            />
          }
          content={
            <LayoutContent>
              <TextInput
                label="单词本名称"
                value={name}
                onChange={setName}
                onEnter={() => importSelected(false)}
                hasAutoFocus
                width="100%"
                status={error ? { type: "error", message: error } : undefined}
                statusVariant="detached"
              />
            </LayoutContent>
          }
          footer={
            <LayoutFooter>
              <Stack direction="horizontal" gap={2} justify="end" wrap="wrap">
                <Button
                  label="取消"
                  variant="secondary"
                  isDisabled={isImporting}
                  onClick={reset}
                />
                <Button
                  label="导入"
                  variant="primary"
                  isLoading={isImporting}
                  onClick={() => importSelected(false)}
                />
              </Stack>
            </LayoutFooter>
          }
        />
      </Dialog>
      <AlertDialog
        isOpen={isConfirming}
        onOpenChange={(isOpen) => {
          if (!isOpen && !isImporting) reset();
        }}
        title="替换同名单词本？"
        description={`“${name.trim()}”已存在。确认后将以新文件中的词条完整替换原内容。`}
        actionLabel="替换单词本"
        actionVariant="primary"
        cancelLabel="取消"
        isActionLoading={isImporting}
        onAction={() => importSelected(true)}
      />
    </>
  );
}

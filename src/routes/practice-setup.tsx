import { Button } from "@astryxdesign/core/Button";
import { Stack } from "@astryxdesign/core/Stack";
import { Text } from "@astryxdesign/core/Text";
import { createFileRoute, useNavigate } from "@tanstack/react-router";
import { useAtom, useAtomValue, useSetAtom } from "jotai";
import { type ReactNode, useEffect, useState } from "react";
import type { WordbookService, WordbookSummary } from "../data/wordbooks";
import { PracticeSetup } from "../features/practice/PracticeSetup";
import { usePracticeStart } from "../features/practice/usePracticeStart";
import {
  activeWordbookAtom,
  countSelectionAtom,
  customCountAtom,
  dispatchRoundAtom,
  practiceSourceAtom,
  roundAtom,
  wordbooksAtom,
} from "../state/appState";

function PracticeSetupPage() {
  const { props } = Route.useRouteContext();
  const service: WordbookService = props.wordbookService;
  const navigate = useNavigate();
  const state = useAtomValue(roundAtom);
  const dispatch = useSetAtom(dispatchRoundAtom);
  const wordbooks = useAtomValue(wordbooksAtom);
  const [activeWordbookId, setActiveWordbookId] = useAtom(activeWordbookAtom);
  const [practiceSource, setPracticeSource] = useAtom(practiceSourceAtom);
  const [countSelection, setCountSelection] = useAtom(countSelectionAtom);
  const [customCount, setCustomCount] = useAtom(customCountAtom);
  const [checkAttempt, setCheckAttempt] = useState(0);
  const [collectionCheck, setCollectionCheck] = useState<{
    wordbooks: WordbookSummary[];
    attempt: number;
    favorites: boolean | Error | null;
    mistakes: boolean | Error | null;
  } | null>(null);
  useEffect(() => {
    if (wordbooks === null || wordbooks.length !== 0) return;
    let current = true;
    const initial = {
      wordbooks,
      attempt: checkAttempt,
      favorites: null,
      mistakes: null,
    };
    const report = (
      source: "favorites" | "mistakes",
      result: boolean | Error,
    ) => {
      if (!current) return;
      setCollectionCheck((previous) => ({
        ...(previous?.wordbooks === wordbooks &&
        previous.attempt === checkAttempt
          ? previous
          : initial),
        [source]: result,
      }));
    };
    const failure = (reason: unknown) =>
      reason instanceof Error ? reason : new Error(String(reason));
    void service.listFavorites().then(
      (entries) => report("favorites", entries.length > 0),
      (reason) => report("favorites", failure(reason)),
    );
    void service.listMistakes().then(
      (entries) => report("mistakes", entries.length > 0),
      (reason) => report("mistakes", failure(reason)),
    );
    return () => {
      current = false;
    };
  }, [wordbooks, checkAttempt, service]);
  const {
    startPractice,
    practiceCount,
    practiceError,
    setPracticeError,
    isStarting,
  } = usePracticeStart();
  if (state.phase !== "setup" || wordbooks === null) return null;
  let setupContent: ReactNode = null;
  if (wordbooks.length === 0) {
    const check =
      collectionCheck?.wordbooks === wordbooks &&
      collectionCheck.attempt === checkAttempt
        ? collectionCheck
        : null;
    // A populated collection need not wait for the other read to finish.
    if (check?.favorites !== true && check?.mistakes !== true) {
      const error =
        check?.favorites instanceof Error
          ? check.favorites
          : check?.mistakes instanceof Error
            ? check.mistakes
            : null;
      const checking =
        check?.favorites === null || check?.mistakes === null || !check;
      setupContent = (
        <Stack gap={3}>
          <Button
            label="返回首页"
            variant="ghost"
            onClick={() => void navigate({ to: "/" })}
          />
          {error ? (
            <>
              <Text role="alert">无法检查收藏夹和错题本：{error.message}</Text>
              <Button
                label="重试"
                onClick={() => setCheckAttempt((n) => n + 1)}
              />
            </>
          ) : checking ? (
            <Text role="status">正在检查收藏夹和错题本…</Text>
          ) : null}
          <Text color="secondary">
            暂无可练习的单词，请返回首页进入单词本管理导入。
          </Text>
        </Stack>
      );
    }
  }
  setupContent ??= (
    <PracticeSetup
      mode={state.mode}
      wordbooks={wordbooks}
      activeWordbookId={activeWordbookId}
      onSelectWordbook={setActiveWordbookId}
      practiceSource={practiceSource}
      onSelectSource={(source) => {
        setPracticeSource(source);
        setPracticeError(null);
      }}
      onBack={() => void navigate({ to: "/" })}
      onSelectMode={(mode) => dispatch({ type: "select-mode", mode })}
      countSelection={countSelection}
      onCountSelectionChange={setCountSelection}
      customCount={customCount}
      onCustomCountChange={setCustomCount}
      practiceCount={practiceCount}
      practiceError={practiceError}
      isStarting={isStarting}
      onStart={() => void startPractice(state.mode)}
    />
  );
  return setupContent;
}

export const Route = createFileRoute("/practice-setup")({
  component: PracticeSetupPage,
});

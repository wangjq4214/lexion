import { Button } from "@astryxdesign/core/Button";
import { Stack } from "@astryxdesign/core/Stack";
import { Text } from "@astryxdesign/core/Text";
import { createFileRoute, useNavigate } from "@tanstack/react-router";
import { useAtom, useAtomValue, useSetAtom } from "jotai";
import { useEffect, useState } from "react";
import type { WordbookService, WordbookSummary } from "../data/wordbooks";
import { PracticeSetup } from "../features/practice/PracticeSetup";
import { usePracticeStart } from "../features/practice/usePracticeStart";
import { WordbookImportFlow } from "../features/wordbooks/WordbookImportFlow";
import { useRefreshWordbooks } from "../features/wordbooks/WordbookProvider";
import {
  activeWordbookAtom,
  countSelectionAtom,
  customCountAtom,
  dispatchRoundAtom,
  practiceSourceAtom,
  roundAtom,
  wordbooksAtom,
} from "../state/appState";

function SetupPage() {
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
  const refreshWordbooks = useRefreshWordbooks();
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
  if (wordbooks.length === 0) {
    const check =
      collectionCheck?.wordbooks === wordbooks &&
      collectionCheck.attempt === checkAttempt
        ? collectionCheck
        : null;
    const importFlow = (
      <WordbookImportFlow
        service={props.wordbookService}
        onImported={(result) => void refreshWordbooks(result.wordbook.id)}
      />
    );
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
      return (
        <Stack gap={3}>
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
          {importFlow}
        </Stack>
      );
    }
  }
  return (
    <PracticeSetup
      mode={state.mode}
      wordbooks={wordbooks}
      wordbookService={props.wordbookService}
      activeWordbookId={activeWordbookId}
      onSelectWordbook={setActiveWordbookId}
      practiceSource={practiceSource}
      onSelectSource={(source) => {
        setPracticeSource(source);
        setPracticeError(null);
      }}
      onOpenFavorites={() => void navigate({ to: "/favorites" })}
      onOpenMistakes={() => void navigate({ to: "/mistakes" })}
      onImported={refreshWordbooks}
      onDeleted={() => refreshWordbooks()}
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
}

export const Route = createFileRoute("/")({ component: SetupPage });

import { createFileRoute, useNavigate } from "@tanstack/react-router";
import { useAtom, useAtomValue, useSetAtom } from "jotai";
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
  const navigate = useNavigate();
  const state = useAtomValue(roundAtom);
  const dispatch = useSetAtom(dispatchRoundAtom);
  const wordbooks = useAtomValue(wordbooksAtom);
  const [activeWordbookId, setActiveWordbookId] = useAtom(activeWordbookAtom);
  const [practiceSource, setPracticeSource] = useAtom(practiceSourceAtom);
  const [countSelection, setCountSelection] = useAtom(countSelectionAtom);
  const [customCount, setCustomCount] = useAtom(customCountAtom);
  const refreshWordbooks = useRefreshWordbooks();
  const {
    startPractice,
    practiceCount,
    practiceError,
    setPracticeError,
    isStarting,
  } = usePracticeStart();
  if (state.phase !== "setup" || wordbooks === null) return null;
  if (wordbooks.length === 0)
    return (
      <WordbookImportFlow
        service={props.wordbookService}
        onImported={(result) => void refreshWordbooks(result.wordbook.id)}
      />
    );
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

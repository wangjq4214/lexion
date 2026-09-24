import { createFileRoute, useNavigate } from "@tanstack/react-router";
import { useAtomValue, useSetAtom } from "jotai";
import { PracticeSummary } from "../features/practice/PracticeSummary";
import { usePracticeStart } from "../features/practice/usePracticeStart";
import { dispatchRoundAtom, roundAtom } from "../state/appState";

function SummaryPage() {
  const state = useAtomValue(roundAtom);
  const dispatch = useSetAtom(dispatchRoundAtom);
  const navigate = useNavigate();
  const { startPractice, practiceError, isStarting } = usePracticeStart();
  if (state.phase !== "summary") return null;
  return (
    <PracticeSummary
      state={state}
      practiceError={practiceError}
      isStarting={isStarting}
      onBack={() => {
        dispatch({ type: "back-to-setup" });
        void navigate({ to: "/" });
      }}
      onRestart={() => void startPractice(state.mode)}
    />
  );
}
export const Route = createFileRoute("/summary")({ component: SummaryPage });

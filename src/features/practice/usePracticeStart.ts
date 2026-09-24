import { useNavigate } from "@tanstack/react-router";
import { useAtom, useAtomValue, useSetAtom } from "jotai";
import { useState } from "react";
import type { WordbookService } from "../../data/wordbooks";
import type { PracticeMode } from "../../domain/practice";
import { Route as RootRoute } from "../../routes/__root";
import {
  activeWordbookAtom,
  countSelectionAtom,
  customCountAtom,
  dispatchRoundAtom,
  practiceSourceAtom,
  roundKeyAtom,
  roundStartedAtAtom,
} from "../../state/appState";

export function usePracticeStart() {
  const { props } = RootRoute.useRouteContext();
  const service: WordbookService = props.wordbookService;
  const now = props.now;
  const navigate = useNavigate();
  const activeWordbookId = useAtomValue(activeWordbookAtom);
  const practiceSource = useAtomValue(practiceSourceAtom);
  const countSelection = useAtomValue(countSelectionAtom);
  const customCount = useAtomValue(customCountAtom);
  const dispatch = useSetAtom(dispatchRoundAtom);
  const setStartedAt = useSetAtom(roundStartedAtAtom);
  const [roundKey, setRoundKey] = useAtom(roundKeyAtom);
  const [practiceError, setPracticeError] = useState<string | null>(null);
  const [isStarting, setIsStarting] = useState(false);
  const value = countSelection === "custom" ? customCount : countSelection;
  const count = /^[0-9]+$/.test(value) ? Number(value) : 0;
  const practiceCount =
    Number.isInteger(count) && count >= 1 && count <= 255 ? count : null;

  const startPractice = async (mode: PracticeMode) => {
    if (activeWordbookId === null || isStarting || practiceCount === null)
      return;
    setIsStarting(true);
    setPracticeError(null);
    try {
      const entries = await service.schedulePractice({
        source: practiceSource,
        wordbookId: practiceSource === "wordbook" ? activeWordbookId : null,
        limit: practiceCount,
        mode,
      });
      if (entries.length === 0) {
        setPracticeError(
          practiceSource === "favorites"
            ? "收藏夹还没有可练习的单词。"
            : practiceSource === "mistakes"
              ? "错题本还没有可练习的单词。"
              : "这个单词本没有可练习的词条。",
        );
        return;
      }
      setRoundKey(roundKey + 1);
      setStartedAt(now());
      dispatch({
        type: "start",
        mode,
        questions: entries.map(({ reviewId, entry, direction }) => ({
          id: String(reviewId),
          reviewId,
          entry,
          direction,
        })),
      });
      void navigate({ to: "/practice", replace: true });
    } catch (error) {
      setPracticeError(error instanceof Error ? error.message : String(error));
    } finally {
      setIsStarting(false);
    }
  };
  return {
    startPractice,
    practiceCount,
    practiceError,
    setPracticeError,
    isStarting,
  };
}

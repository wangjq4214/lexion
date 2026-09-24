import { atom } from "jotai";
import type { WordbookSummary } from "../data/wordbooks";
import type { PracticeSource } from "../domain/practice";
import {
  type AppAction,
  appReducer,
  initialState,
} from "../features/practice/practiceReducer";

export const roundAtom = atom(initialState);
export const dispatchRoundAtom = atom(null, (get, set, action: AppAction) => {
  set(roundAtom, appReducer(get(roundAtom), action));
});
export const activeWordbookAtom = atom<number | null>(null);
export const practiceSourceAtom = atom<PracticeSource>("wordbook");
export const countSelectionAtom = atom("10");
export const customCountAtom = atom("");
export const wordbooksAtom = atom<WordbookSummary[] | null>(null);
export const wordbooksErrorAtom = atom<string | null>(null);
export const roundStartedAtAtom = atom<number | null>(null);
export const roundKeyAtom = atom(0);
export const favoritesVersionAtom = atom(0);
export const favoriteWriteErrorAtom = atom<string | null>(null);

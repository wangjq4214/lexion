import type { PracticeMode } from "../../domain/practice";

export const modeLabels: Record<PracticeMode, string> = {
  "zh-to-en": "看中文拼英文",
  "en-to-zh": "看英文拼中文",
  mixed: "双向混合随机",
};

export function formatElapsedTime(elapsedSeconds: number) {
  const minutes = Math.floor(elapsedSeconds / 60);
  const seconds = elapsedSeconds % 60;
  return `${minutes}:${String(seconds).padStart(2, "0")}`;
}

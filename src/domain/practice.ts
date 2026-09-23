import type { WordEntry } from "./word";

export type PracticeMode = "zh-to-en" | "en-to-zh" | "mixed";
export type PracticeSource = "wordbook" | "favorites" | "mistakes";
export type QuestionDirection = Exclude<PracticeMode, "mixed">;
export type RandomSource = () => number;

export type Question = {
  id: string;
  reviewId?: number;
  entry: WordEntry;
  direction: QuestionDirection;
};

function randomIndex(length: number, random: RandomSource): number {
  const value = Math.min(Math.max(random(), 0), 1 - Number.EPSILON);
  return Math.floor(value * length);
}

export function createQuestions(
  entries: readonly WordEntry[],
  mode: PracticeMode,
  random: RandomSource = Math.random,
): Question[] {
  const shuffled = [...entries];

  for (let index = shuffled.length - 1; index > 0; index -= 1) {
    const swapIndex = randomIndex(index + 1, random);
    [shuffled[index], shuffled[swapIndex]] = [
      shuffled[swapIndex],
      shuffled[index],
    ];
  }

  return shuffled.map((entry) => ({
    id: String(entry.id),
    entry,
    direction:
      mode === "mixed" ? (random() < 0.5 ? "zh-to-en" : "en-to-zh") : mode,
  }));
}

export function getQuestionPrompt(question: Question): string {
  return question.direction === "zh-to-en"
    ? question.entry.chinese
    : question.entry.english;
}

export function getExpectedAnswer(question: Question): string {
  return question.direction === "zh-to-en"
    ? question.entry.english
    : question.entry.chinese;
}

export function isExactAnswer(input: string, question: Question): boolean {
  const normalizedInput = input.trim();
  const expectedAnswer = getExpectedAnswer(question);

  if (question.direction === "zh-to-en") {
    return (
      normalizedInput.toLocaleLowerCase("en-US") ===
      expectedAnswer.toLocaleLowerCase("en-US")
    );
  }

  return normalizedInput === expectedAnswer;
}

export function createEnglishHints(
  word: string,
  random: RandomSource = Math.random,
): [string, string] {
  const characters = Array.from(word);
  const letterIndexes = characters
    .map((character, index) => (/^[a-z]$/i.test(character) ? index : -1))
    .filter((index) => index >= 0);

  for (let index = letterIndexes.length - 1; index > 0; index -= 1) {
    const swapIndex = randomIndex(index + 1, random);
    [letterIndexes[index], letterIndexes[swapIndex]] = [
      letterIndexes[swapIndex],
      letterIndexes[index],
    ];
  }

  const maxVisible = Math.max(0, letterIndexes.length - 1);
  const formatHint = (fraction: number): string => {
    const visibleCount = Math.min(
      maxVisible,
      Math.round(letterIndexes.length * fraction),
    );
    const visibleIndexes = new Set(letterIndexes.slice(0, visibleCount));
    return characters
      .map((character, index) =>
        /^[a-z]$/i.test(character) && !visibleIndexes.has(index)
          ? "_"
          : character,
      )
      .join(" ");
  };

  return [formatHint(1 / 3), formatHint(2 / 3)];
}

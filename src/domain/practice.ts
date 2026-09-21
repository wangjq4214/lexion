import type { WordEntry } from "../data/vocabulary";

export type PracticeMode = "zh-to-en" | "en-to-zh" | "mixed";
export type QuestionDirection = Exclude<PracticeMode, "mixed">;
export type RandomSource = () => number;

export type Question = {
  id: string;
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
    id: entry.id,
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

export function createEnglishHint(
  word: string,
  random: RandomSource = Math.random,
): string {
  const characters = Array.from(word);
  const letterIndexes = characters
    .map((character, index) => (/^[a-z]$/i.test(character) ? index : -1))
    .filter((index) => index >= 0);

  if (letterIndexes.length === 0) {
    return word;
  }

  const maskedIndexes = new Set(letterIndexes.filter(() => random() < 0.5));

  if (maskedIndexes.size === 0) {
    maskedIndexes.add(letterIndexes[randomIndex(letterIndexes.length, random)]);
  }

  if (maskedIndexes.size === letterIndexes.length && letterIndexes.length > 1) {
    maskedIndexes.delete(
      letterIndexes[randomIndex(letterIndexes.length, random)],
    );
  }

  return characters
    .map((character, index) => (maskedIndexes.has(index) ? "_" : character))
    .join(" ");
}

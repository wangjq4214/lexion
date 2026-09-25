import { getExpectedAnswer, isExactAnswer, type Question } from "./practice";

export type ExamItem = Pick<Question, "entry" | "direction">;

export function gradeExam(
  questions: readonly ExamItem[],
  answers: readonly string[],
) {
  if (questions.length === 0 || answers.length !== questions.length) {
    throw new Error("考试题目与答案数量不一致。");
  }
  const results = questions.map((item, index) => {
    const question: Question = { ...item, id: String(index) };
    return {
      correct: isExactAnswer(answers[index], question),
      expected: getExpectedAnswer(question),
      answer: answers[index],
    };
  });
  const correctCount = results.filter((result) => result.correct).length;
  return {
    results,
    correctCount,
    score: (correctCount / questions.length) * 100,
  };
}

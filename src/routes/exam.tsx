import { AlertDialog } from "@astryxdesign/core/AlertDialog";
import { Button } from "@astryxdesign/core/Button";
import { Heading } from "@astryxdesign/core/Heading";
import { List, ListItem } from "@astryxdesign/core/List";
import { Section } from "@astryxdesign/core/Section";
import { Selector } from "@astryxdesign/core/Selector";
import { Stack } from "@astryxdesign/core/Stack";
import { Text } from "@astryxdesign/core/Text";
import { TextInput } from "@astryxdesign/core/TextInput";
import {
  createFileRoute,
  useBlocker,
  useNavigate,
} from "@tanstack/react-router";
import { useAtomValue } from "jotai";
import { useRef, useState } from "react";
import type {
  ExamQuestion,
  ExamRequest,
  WordbookService,
} from "../data/wordbooks";
import { gradeExam } from "../domain/exam";
import { getQuestionPrompt } from "../domain/practice";
import {
  activeWordbookAtom,
  practiceSourceAtom,
  wordbooksAtom,
} from "../state/appState";

function countValue(input: string): number | null {
  if (!/^(0|[1-9][0-9]*)$/.test(input)) return null;
  const count = Number(input);
  return Number.isSafeInteger(count) && count <= 4294967295 ? count : null;
}

function newSubmissionId(): string {
  return `exam:${Array.from(
    crypto.getRandomValues(new Uint8Array(16)),
    (byte) => byte.toString(16).padStart(2, "0"),
  ).join("")}`;
}

async function saveWithTimeout(write: Promise<unknown>): Promise<void> {
  let timeout: ReturnType<typeof setTimeout> | undefined;
  try {
    await Promise.race([
      write,
      new Promise<never>((_, reject) => {
        timeout = setTimeout(() => reject(new Error("保存超时")), 10_000);
      }),
    ]);
  } finally {
    if (timeout !== undefined) clearTimeout(timeout);
  }
}

type ExamSession = {
  questions: ExamQuestion[];
  answers: string[];
  grade: ReturnType<typeof gradeExam> | null;
  submissionIds: string[];
};

function ExamPage() {
  const { props } = Route.useRouteContext();
  const service: WordbookService = props.wordbookService;
  const navigate = useNavigate();
  const wordbooks = useAtomValue(wordbooksAtom) ?? [];
  const activeWordbookId = useAtomValue(activeWordbookAtom);
  const practiceSource = useAtomValue(practiceSourceAtom);
  const [source, setSource] = useState<ExamRequest["source"]>(
    wordbooks.length ? practiceSource : "favorites",
  );
  const [wordbookId, setWordbookId] = useState<number | null>(
    activeWordbookId ?? wordbooks[0]?.id ?? null,
  );
  const [enToZh, setEnToZh] = useState("5");
  const [zhToEn, setZhToEn] = useState("5");
  const [session, setSession] = useState<ExamSession | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [isStarting, setIsStarting] = useState(false);
  const [isSaving, setIsSaving] = useState(false);
  const [confirmExit, setConfirmExit] = useState(false);
  const saving = useRef(false);
  const allowExit = useRef(false);
  useBlocker({
    shouldBlockFn: ({ next }) => {
      if (allowExit.current || next.pathname === "/exam" || !session)
        return false;
      if (session.grade && !isSaving && !error) return false;
      if (!session.grade) setConfirmExit(true);
      return true;
    },
  });
  const saved = useRef(new Set<number>());
  const enCount = countValue(enToZh);
  const zhCount = countValue(zhToEn);
  const validCounts =
    enCount !== null &&
    zhCount !== null &&
    enCount + zhCount > 0 &&
    enCount + zhCount <= 4294967295;

  const start = async () => {
    if (isStarting || !validCounts || enCount === null || zhCount === null)
      return;
    if (source === "wordbook" && wordbookId === null) return;
    setIsStarting(true);
    setError(null);
    try {
      const questions = await service.sampleExam({
        source,
        wordbookId: source === "wordbook" ? wordbookId : null,
        enToZhCount: enCount,
        zhToEnCount: zhCount,
      });
      if (questions.length !== enCount + zhCount) {
        throw new Error("考试题数不足，请减少题数后重试。");
      }
      saved.current = new Set();
      setSession({
        questions,
        answers: questions.map(() => ""),
        grade: null,
        submissionIds: questions.map(() => newSubmissionId()),
      });
    } catch (reason) {
      setError(reason instanceof Error ? reason.message : String(reason));
    } finally {
      setIsStarting(false);
    }
  };

  const saveMistakes = async (
    current: ExamSession,
    grade: ReturnType<typeof gradeExam>,
  ) => {
    if (saving.current) return;
    saving.current = true;
    setIsSaving(true);
    setError(null);
    try {
      for (let index = 0; index < grade.results.length; index += 1) {
        if (grade.results[index].correct || saved.current.has(index)) continue;
        const { english, chinese } = current.questions[index].entry;
        await saveWithTimeout(
          service.recordMistakeOnce(
            english,
            chinese,
            current.submissionIds[index],
          ),
        );
        saved.current.add(index);
      }
    } catch (reason) {
      setError(
        `错题保存失败，请重试：${reason instanceof Error ? reason.message : String(reason)}`,
      );
    } finally {
      saving.current = false;
      setIsSaving(false);
    }
  };

  const check = () => {
    if (!session || session.grade || saving.current) return;
    const grade = gradeExam(session.questions, session.answers);
    setSession({ ...session, grade });
    void saveMistakes(session, grade);
  };

  const goBack = () => {
    if (session?.grade && (saving.current || error)) return;
    allowExit.current = true;
    setConfirmExit(false);
    void navigate({ to: "/" });
  };

  return (
    <Stack gap={6}>
      <Section paddingBlockStart={6} paddingBlockEnd={0}>
        <Stack direction="horizontal" gap={3} justify="between" wrap="wrap">
          <Stack gap={2}>
            <Heading level={1}>单词考试</Heading>
            <Text color="secondary">
              先完成所有题目，再统一核对答案与成绩。
            </Text>
          </Stack>
          {session === null || session.grade !== null ? (
            <Button
              label="返回首页"
              size="sm"
              variant="ghost"
              isDisabled={
                isStarting || isSaving || (session !== null && error !== null)
              }
              onClick={goBack}
            />
          ) : null}
        </Stack>
      </Section>
      {session === null ? (
        <Section>
          <Stack gap={4}>
            <Heading level={2}>考试设置</Heading>
            <Selector
              label="考试来源"
              options={[
                {
                  value: "wordbook",
                  label: "单词本",
                  disabled: wordbooks.length === 0,
                },
                { value: "favorites", label: "收藏夹" },
                { value: "mistakes", label: "错题本" },
              ]}
              value={source}
              onChange={(value) => {
                setSource(value as ExamRequest["source"]);
                setError(null);
              }}
              width="100%"
            />
            {source === "wordbook" ? (
              <Selector
                label="考试单词本"
                options={wordbooks.map((book) => ({
                  value: String(book.id),
                  label: book.name,
                  description: `${book.entryCount} 个词条`,
                }))}
                value={wordbookId === null ? undefined : String(wordbookId)}
                onChange={(value) => setWordbookId(Number(value))}
                width="100%"
              />
            ) : null}
            <TextInput
              label="英译汉题数"
              value={enToZh}
              onChange={setEnToZh}
              width="100%"
            />
            <TextInput
              label="汉译英题数"
              value={zhToEn}
              onChange={setZhToEn}
              width="100%"
            />
            {!validCounts ? (
              <Text role="alert">请输入非负整数，且总题数至少为 1。</Text>
            ) : null}
            {error ? <Text role="alert">{error}</Text> : null}
            <Button
              label="开始考试"
              variant="primary"
              isLoading={isStarting}
              isDisabled={
                !validCounts || (source === "wordbook" && wordbookId === null)
              }
              onClick={() => void start()}
            />
          </Stack>
        </Section>
      ) : (
        <Section padding={0}>
          <Stack gap={4}>
            {session.grade ? (
              <Section>
                <Stack gap={2}>
                  <Heading level={2}>
                    考试成绩：{Number(session.grade.score.toFixed(2))} 分
                  </Heading>
                  <Text>
                    答对 {session.grade.correctCount} /{" "}
                    {session.questions.length} 题
                  </Text>
                  {isSaving ? <Text role="status">正在保存错题…</Text> : null}
                  {error ? <Text role="alert">{error}</Text> : null}
                  {error && !isSaving ? (
                    <Button
                      label="重试保存错题"
                      onClick={() => {
                        if (session.grade)
                          void saveMistakes(session, session.grade);
                      }}
                    />
                  ) : null}
                </Stack>
              </Section>
            ) : null}
            <List header={<Heading level={2}>考试题目</Heading>} hasDividers>
              {session.questions.map((question, index) => {
                const graded = session.grade?.results[index];
                const prompt = getQuestionPrompt({
                  ...question,
                  id: String(index),
                });
                return (
                  <ListItem
                    key={`${question.direction}:${question.entry.id}`}
                    label={`${index + 1}. ${prompt}`}
                    description={
                      <Stack gap={2}>
                        <Text color="secondary">
                          {question.direction === "en-to-zh"
                            ? "英译汉"
                            : "汉译英"}
                        </Text>
                        <TextInput
                          label={`第 ${index + 1} 题答案`}
                          value={session.answers[index]}
                          onChange={(value) =>
                            setSession((previous) => {
                              if (!previous || previous.grade) return previous;
                              const answers = [...previous.answers];
                              answers[index] = value;
                              return { ...previous, answers };
                            })
                          }
                          isReadOnly={session.grade !== null}
                          width="100%"
                        />
                        {graded ? (
                          <Text>
                            {graded.correct ? "正确" : "错误"}；正确答案：
                            {graded.expected}
                          </Text>
                        ) : null}
                      </Stack>
                    }
                  />
                );
              })}
            </List>
            {session.grade === null ? (
              <Section>
                <Stack direction="horizontal" gap={2} wrap="wrap">
                  <Button label="统一核对" variant="primary" onClick={check} />
                  <Button
                    label="放弃考试"
                    variant="ghost"
                    isDisabled={isSaving}
                    onClick={() => setConfirmExit(true)}
                  />
                </Stack>
              </Section>
            ) : null}
          </Stack>
        </Section>
      )}
      <AlertDialog
        isOpen={confirmExit}
        onOpenChange={setConfirmExit}
        title="放弃这场考试？"
        description="未核对的答案将丢失，且不会计入错题本。"
        actionLabel="放弃考试"
        cancelLabel="继续答题"
        onAction={goBack}
      />
    </Stack>
  );
}

export const Route = createFileRoute("/exam")({ component: ExamPage });

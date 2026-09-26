import { Button } from "@astryxdesign/core/Button";
import { ClickableCard } from "@astryxdesign/core/ClickableCard";
import { Grid } from "@astryxdesign/core/Grid";
import { Heading } from "@astryxdesign/core/Heading";
import { Section } from "@astryxdesign/core/Section";
import { Stack } from "@astryxdesign/core/Stack";
import { Text } from "@astryxdesign/core/Text";
import { createFileRoute, useNavigate } from "@tanstack/react-router";

function HomePage() {
  const navigate = useNavigate();
  return (
    <Stack gap={6}>
      <Section paddingBlockStart={6} paddingBlockEnd={0}>
        <Stack gap={2}>
          <Heading level={1}>Lexicon 单词学习</Heading>
          <Text color="secondary">选择练习或考试，进入后再设置具体模式。</Text>
        </Stack>
      </Section>
      <Grid columns={{ minWidth: 240, max: 2, repeat: "fit" }} gap={4}>
        <ClickableCard
          label="练习"
          padding={8}
          elevation="low"
          onClick={() => void navigate({ to: "/practice-setup" })}
        >
          <Stack gap={2}>
            <Heading level={2}>练习</Heading>
            <Text color="secondary">按自己的节奏巩固单词</Text>
          </Stack>
        </ClickableCard>
        <ClickableCard
          label="考试"
          padding={8}
          elevation="low"
          onClick={() => void navigate({ to: "/exam" })}
        >
          <Stack gap={2}>
            <Heading level={2}>考试</Heading>
            <Text color="secondary">完成题目后统一核对成绩</Text>
          </Stack>
        </ClickableCard>
      </Grid>
      <Section variant="transparent" padding={0}>
        <Stack direction="horizontal" gap={2} wrap="wrap" justify="center">
          <Button
            label="错题本"
            variant="ghost"
            size="sm"
            onClick={() => void navigate({ to: "/mistakes" })}
          />
          <Button
            label="收藏夹"
            variant="ghost"
            size="sm"
            onClick={() => void navigate({ to: "/favorites" })}
          />
          <Button
            label="单词本管理"
            variant="ghost"
            size="sm"
            onClick={() => void navigate({ to: "/wordbooks" })}
          />
          <Button
            label="局域网设备配对"
            variant="ghost"
            size="sm"
            onClick={() => void navigate({ to: "/devices" })}
          />
        </Stack>
      </Section>
    </Stack>
  );
}

export const Route = createFileRoute("/")({ component: HomePage });

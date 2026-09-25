import { Button } from "@astryxdesign/core/Button";
import { Heading } from "@astryxdesign/core/Heading";
import { Section } from "@astryxdesign/core/Section";
import { Stack } from "@astryxdesign/core/Stack";
import { Text } from "@astryxdesign/core/Text";
import { createFileRoute, useNavigate } from "@tanstack/react-router";
import { useState } from "react";
import { WordbookImportFlow } from "../features/wordbooks/WordbookImportFlow";
import { useRefreshWordbooks } from "../features/wordbooks/WordbookProvider";

function ImportPage() {
  const { props } = Route.useRouteContext();
  const navigate = useNavigate();
  const refreshWordbooks = useRefreshWordbooks();
  const [importedName, setImportedName] = useState<string | null>(null);
  return (
    <Stack gap={4}>
      <Button
        label="返回首页"
        variant="ghost"
        onClick={() => void navigate({ to: "/" })}
      />
      <Section>
        <Stack gap={4}>
          <Heading level={1}>导入单词本</Heading>
          <Text color="secondary">从 Excel 文件导入单词本。</Text>
          <WordbookImportFlow
            service={props.wordbookService}
            onImported={async (result) => {
              await refreshWordbooks(result.wordbook.id);
              setImportedName(result.wordbook.name);
            }}
          />
          {importedName ? (
            <Text role="status">已导入“{importedName}”。</Text>
          ) : null}
        </Stack>
      </Section>
    </Stack>
  );
}

export const Route = createFileRoute("/import")({ component: ImportPage });

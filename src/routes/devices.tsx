import { Button } from "@astryxdesign/core/Button";
import { Heading } from "@astryxdesign/core/Heading";
import { List, ListItem } from "@astryxdesign/core/List";
import { Section } from "@astryxdesign/core/Section";
import { Stack } from "@astryxdesign/core/Stack";
import { Text } from "@astryxdesign/core/Text";
import { TextInput } from "@astryxdesign/core/TextInput";
import { createFileRoute, useNavigate } from "@tanstack/react-router";
import { useCallback, useEffect, useState } from "react";
import { type LanStatus, lanService } from "../data/lan";

function DevicesPage() {
  const navigate = useNavigate();
  const [status, setStatus] = useState<LanStatus | null>(null);
  const [loadError, setLoadError] = useState<string | null>(null);
  const [actionError, setActionError] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);
  const [codes, setCodes] = useState<Record<string, string>>({});
  const refresh = useCallback(async () => {
    try {
      setStatus(await lanService.status());
      setLoadError(null);
    } catch (error) {
      setLoadError(error instanceof Error ? error.message : String(error));
    }
  }, []);
  useEffect(() => {
    const initial = window.setTimeout(() => void refresh(), 0);
    const timer = window.setInterval(() => void refresh(), 1500);
    return () => {
      window.clearTimeout(initial);
      window.clearInterval(timer);
    };
  }, [refresh]);
  const act = async (action: () => Promise<void>) => {
    setBusy(true);
    setActionError(null);
    try {
      await action();
      await refresh();
    } catch (error) {
      setActionError(error instanceof Error ? error.message : String(error));
    } finally {
      setBusy(false);
    }
  };

  return (
    <Stack gap={6}>
      <Section paddingBlockStart={6} paddingBlockEnd={0}>
        <Stack direction="horizontal" justify="between" gap={3} wrap="wrap">
          <Heading level={1}>局域网设备配对</Heading>
          <Button
            label="返回首页"
            variant="ghost"
            onClick={() => void navigate({ to: "/" })}
          />
        </Stack>
      </Section>
      <Text color="secondary">
        首次配对请在两台设备上核对相同的八位数字，并分别输入对方屏幕上的数字确认。配对仅授权这两台设备；学习数据交换将在后续版本提供。
      </Text>
      {loadError ? <Text role="alert">连接服务失败：{loadError}</Text> : null}
      {actionError ? (
        <Text role="alert">配对操作失败：{actionError}</Text>
      ) : null}
      {status?.error ? (
        <Text role="alert">
          局域网不可用：{status.error}。本地学习不受影响。
        </Text>
      ) : null}
      <Section>
        <Stack gap={3}>
          <Stack direction="horizontal" justify="between" gap={2} wrap="wrap">
            <Heading level={2}>附近设备</Heading>
            <Button
              label="刷新设备"
              size="sm"
              variant="ghost"
              onClick={() => void refresh()}
            />
          </Stack>
          {status === null ? (
            <Text role="status">正在搜索局域网设备…</Text>
          ) : status.peers.length === 0 ? (
            <Text>
              暂未发现设备。请确认两台设备在同一局域网且允许 mDNS
              与本应用通过防火墙。本地练习仍可继续。
            </Text>
          ) : (
            <List header={<Heading level={3}>发现的设备</Heading>} hasDividers>
              {status.peers.map((peer) => (
                <ListItem
                  key={peer.id}
                  label={peer.name}
                  description={peer.trusted ? "已配对" : "尚未配对"}
                  endContent={
                    peer.trusted ? undefined : (
                      <Button
                        label={`与 ${peer.name} 建立配对`}
                        size="sm"
                        variant="secondary"
                        isDisabled={busy}
                        onClick={() => void act(() => lanService.pair(peer.id))}
                      />
                    )
                  }
                />
              ))}
            </List>
          )}
        </Stack>
      </Section>
      {status && status.pending.length > 0 ? (
        <Section>
          <Stack gap={4}>
            <Heading level={2}>等待双方确认</Heading>
            {status.pending.map((request) => (
              <Section key={request.id} variant="muted">
                <Stack gap={3}>
                  <Heading level={3}>{request.name}</Heading>
                  <Text>本机配对码：{request.code}</Text>
                  <Text color="secondary">
                    请口头核对对方屏幕的配对码与本机一致；不一致时立即取消。
                  </Text>
                  <TextInput
                    label={`输入 ${request.name} 屏幕上的八位配对码`}
                    value={codes[request.id] ?? ""}
                    onChange={(value) =>
                      setCodes((previous) => ({
                        ...previous,
                        [request.id]: value,
                      }))
                    }
                  />
                  <Stack direction="horizontal" gap={2} wrap="wrap">
                    <Button
                      label="双方数字一致，确认配对"
                      variant="primary"
                      isDisabled={
                        busy || !/^\d{8}$/.test(codes[request.id] ?? "")
                      }
                      onClick={() =>
                        void act(() =>
                          lanService.confirm(
                            request.id,
                            codes[request.id] ?? "",
                          ),
                        )
                      }
                    />
                    <Button
                      label="取消配对"
                      variant="ghost"
                      isDisabled={busy}
                      onClick={() =>
                        void act(() => lanService.cancel(request.id))
                      }
                    />
                  </Stack>
                </Stack>
              </Section>
            ))}
          </Stack>
        </Section>
      ) : null}
      {status && status.trusted.length > 0 ? (
        <Section>
          <List header={<Heading level={2}>已授权设备</Heading>} hasDividers>
            {status.trusted.map((peer) => (
              <ListItem
                key={peer.id}
                label={peer.name}
                description="已直接配对；再次相遇无需重新核对"
              />
            ))}
          </List>
        </Section>
      ) : null}
    </Stack>
  );
}

export const Route = createFileRoute("/devices")({ component: DevicesPage });

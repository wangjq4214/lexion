import { AppShell } from "@astryxdesign/core/AppShell";
import { Button } from "@astryxdesign/core/Button";
import { Layout, LayoutContent } from "@astryxdesign/core/Layout";
import { Stack } from "@astryxdesign/core/Stack";
import { Text } from "@astryxdesign/core/Text";
import * as stylex from "@stylexjs/stylex";
import {
  createBrowserHistory,
  createMemoryHistory,
  createRouter,
  Outlet,
  type RouterHistory,
  RouterProvider,
  useNavigate,
  useRouterState,
} from "@tanstack/react-router";
import { Provider, useAtomValue, useSetAtom } from "jotai";
import { createStore } from "jotai/vanilla";
import { useEffect, useLayoutEffect, useState } from "react";
import { tauriWordbookService, type WordbookService } from "./data/wordbooks";
import type { RandomSource } from "./domain/practice";
import {
  useRefreshWordbooks,
  WordbookProvider,
} from "./features/wordbooks/WordbookProvider";
import { routeTree } from "./routeTree.gen";
import {
  dispatchRoundAtom,
  favoriteWriteErrorAtom,
  roundAtom,
  wordbooksAtom,
  wordbooksErrorAtom,
} from "./state/appState";
import { tokens } from "./styles/tokens.stylex";

const styles = stylex.create({
  documentRoot: {
    MozOsxFontSmoothing: "grayscale",
    WebkitFontSmoothing: "antialiased",
    fontSynthesis: "none",
    backgroundColor: tokens.appBackground,
    color: tokens.appText,
    fontFamily: "var(--font-family-body)",
    fontSize: "var(--text-body-size)",
    fontWeight: "var(--text-body-weight)",
    lineHeight: "var(--text-body-leading)",
    textRendering: "optimizeLegibility",
    textSizeAdjust: "100%",
  },
});

export type AppProps = {
  random?: RandomSource;
  now?: () => number;
  wordbookService?: WordbookService;
  history?: RouterHistory;
};

function RouteFrame() {
  const wordbooks = useAtomValue(wordbooksAtom);
  const favoriteWriteError = useAtomValue(favoriteWriteErrorAtom);
  const setFavoriteWriteError = useSetAtom(favoriteWriteErrorAtom);
  const error = useAtomValue(wordbooksErrorAtom);
  const refresh = useRefreshWordbooks();
  const round = useAtomValue(roundAtom);
  const dispatch = useSetAtom(dispatchRoundAtom);
  const navigate = useNavigate();
  const pathname = useRouterState({
    select: (state) => state.location.pathname,
  });
  useEffect(() => {
    if (
      round.phase === "setup" &&
      (pathname === "/practice" || pathname === "/summary")
    ) {
      void navigate({ to: "/", replace: true });
    } else if (round.phase === "practice" && pathname !== "/practice") {
      void navigate({ to: "/practice", replace: true });
    } else if (round.phase === "summary" && pathname === "/practice") {
      void navigate({ to: "/summary", replace: true });
    } else if (round.phase === "summary" && pathname !== "/summary") {
      dispatch({ type: "back-to-setup" });
    }
  }, [round.phase, pathname, navigate, dispatch]);
  return (
    <AppShell variant="surface" contentPadding={0} mobileNav={false}>
      <Layout
        contentWidth={640}
        height="fill"
        padding={6}
        content={
          <LayoutContent label="单词练习主内容">
            <Stack gap={4}>
              {favoriteWriteError && pathname !== "/practice" ? (
                <Stack gap={2}>
                  <Text role="alert">{favoriteWriteError}</Text>
                  <Button
                    label="关闭收藏错误提示"
                    variant="ghost"
                    onClick={() => setFavoriteWriteError(null)}
                  />
                </Stack>
              ) : null}
              {error !== null ? (
                <Stack gap={4}>
                  <Text role="alert">无法加载单词本：{error}</Text>
                  <Button
                    label="重试"
                    variant="primary"
                    onClick={() => void refresh()}
                  />
                </Stack>
              ) : wordbooks === null ? (
                <Text role="status">正在加载单词本…</Text>
              ) : (
                <Outlet />
              )}
            </Stack>
          </LayoutContent>
        }
      />
    </AppShell>
  );
}

export function AppHost({ wordbookService = tauriWordbookService }: AppProps) {
  useLayoutEffect(() => {
    const { className } = stylex.props(styles.documentRoot);
    const names = className?.split(" ") ?? [];
    document.documentElement.classList.add(...names);
    return () => document.documentElement.classList.remove(...names);
  }, []);
  return (
    <WordbookProvider service={wordbookService}>
      <RouteFrame />
    </WordbookProvider>
  );
}

export default function App(props: AppProps) {
  const [store] = useState(createStore);
  const [router] = useState(() =>
    createRouter({
      routeTree,
      history:
        props.history ??
        (import.meta.env.MODE === "test"
          ? createMemoryHistory({ initialEntries: [window.location.pathname] })
          : createBrowserHistory()),
      context: {
        props: {
          ...props,
          wordbookService: props.wordbookService ?? tauriWordbookService,
          now: props.now ?? (() => performance.now()),
          random: props.random ?? Math.random,
        },
      },
      defaultPreload: false,
    }),
  );
  return (
    <Provider store={store}>
      <RouterProvider router={router} />
    </Provider>
  );
}

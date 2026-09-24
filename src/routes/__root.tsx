import { createRootRouteWithContext } from "@tanstack/react-router";
import { AppHost, type AppProps } from "../App";
import type { WordbookService } from "../data/wordbooks";
import type { RandomSource } from "../domain/practice";

function RootPage() {
  const { props } = Route.useRouteContext();
  return <AppHost {...props} />;
}

export const Route = createRootRouteWithContext<{
  props: AppProps & {
    wordbookService: WordbookService;
    now: () => number;
    random: RandomSource;
  };
}>()({
  component: RootPage,
});

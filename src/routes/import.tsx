import { createFileRoute, redirect } from "@tanstack/react-router";

// Keep existing deep links usable without exposing a second management screen.
export const Route = createFileRoute("/import")({
  beforeLoad: () => {
    throw redirect({ to: "/wordbooks", replace: true });
  },
});

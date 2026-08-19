import { createFileRoute, redirect } from "@tanstack/react-router";

export const Route = createFileRoute("/org")({
  beforeLoad: () => {
    throw redirect({ to: "/" });
  },
});

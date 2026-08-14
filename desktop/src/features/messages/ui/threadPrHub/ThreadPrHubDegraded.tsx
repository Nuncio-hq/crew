import * as React from "react";

import { Button } from "@/shared/ui/button";
import type { ForgeAvailability } from "@/shared/api/threadForgeTypes";

export function ThreadPrHubDegraded({
  availability,
  message,
  onRecheck,
  rateLimitedUntil,
  refreshDisabled,
}: {
  availability: ForgeAvailability;
  message: string | null;
  onRecheck: () => void;
  rateLimitedUntil: string | null;
  refreshDisabled: boolean;
}) {
  if (availability === "cli-missing") {
    return (
      <div
        className="flex flex-1 flex-col items-start gap-3 p-4"
        data-testid="thread-pr-hub-cli-missing"
      >
        <h2 className="text-sm font-semibold">GitHub CLI is not installed</h2>
        <p className="text-sm text-muted-foreground">
          The PR hub reads GitHub through <code>gh</code>. Install it, then
          authenticate.
        </p>
        <pre className="w-full overflow-x-auto rounded-md bg-muted p-2 font-mono text-2xs">
          brew install gh{"\n"}gh auth login
        </pre>
        <Button
          data-testid="thread-pr-hub-recheck"
          onClick={onRecheck}
          size="sm"
          type="button"
        >
          Recheck
        </Button>
      </div>
    );
  }
  if (availability === "rate-limited") {
    return (
      <div
        className="m-3 rounded-md border border-attention/40 bg-attention/10 p-3"
        data-testid="thread-pr-hub-rate-limited"
      >
        <p className="text-sm font-medium text-attention dark:text-attention">
          GitHub rate limit reached
        </p>
        <p className="mt-1 text-2xs text-muted-foreground">
          <RateLimitCountdown until={rateLimitedUntil} />
        </p>
        <Button
          className="mt-2"
          data-testid="thread-pr-hub-recheck"
          disabled={refreshDisabled}
          onClick={onRecheck}
          size="xs"
          type="button"
          variant="outline"
        >
          Retry
        </Button>
      </div>
    );
  }
  return (
    <div
      className="flex flex-1 flex-col items-start gap-3 p-4"
      data-testid="thread-pr-hub-cli-failed"
    >
      <h2 className="text-sm font-semibold">
        Could not read this pull request
      </h2>
      <p className="text-sm text-muted-foreground">
        {message ??
          "gh could not complete the request. Check auth and try again."}
      </p>
      <pre className="w-full overflow-x-auto rounded-md bg-muted p-2 font-mono text-2xs">
        gh auth status
      </pre>
      <Button
        data-testid="thread-pr-hub-recheck"
        onClick={onRecheck}
        size="sm"
        type="button"
      >
        Recheck
      </Button>
    </div>
  );
}

function RateLimitCountdown({ until }: { until: string | null }) {
  const [now, setNow] = React.useState(Date.now());
  React.useEffect(() => {
    const id = window.setInterval(() => setNow(Date.now()), 1000);
    return () => window.clearInterval(id);
  }, []);
  if (!until) return <>Wait a bit, then refresh.</>;
  const remaining = Math.max(0, Date.parse(until) - now);
  if (Number.isNaN(remaining)) return <>Retry after {until}.</>;
  const seconds = Math.ceil(remaining / 1000);
  if (seconds <= 0) return <>You can retry now.</>;
  const minutes = Math.floor(seconds / 60);
  const rest = seconds % 60;
  return (
    <>
      Retry in {minutes > 0 ? `${minutes}m ${rest}s` : `${seconds}s`} ({until}).
    </>
  );
}

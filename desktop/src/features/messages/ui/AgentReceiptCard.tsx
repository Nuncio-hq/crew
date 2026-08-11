import type { AgentReceiptModel } from "@/features/messages/lib/agentReceipt.mjs";

type AgentReceiptCardProps = {
  disabled?: boolean;
  onRequestChanges?: () => void;
  onReviewed?: () => void;
  receipt: AgentReceiptModel;
  reviewed?: boolean;
};

export function resolvePrReferenceHref(value: string) {
  if (/^https?:\/\//i.test(value)) {
    return value;
  }

  const match = /^(?:([^/#\s]+\/[^/#\s]+))?#(\d+)$/.exec(value);
  if (!match) {
    return null;
  }

  return `https://github.com/${match[1] ?? "Nuncio-hq/crew"}/pull/${match[2]}`;
}

function ExternalReference({ value }: { value: string }) {
  const href = resolvePrReferenceHref(value);

  if (href) {
    return (
      <a
        className="text-primary underline underline-offset-2"
        href={href}
        rel="noreferrer"
        target="_blank"
      >
        {value}
      </a>
    );
  }
  return <span>{value}</span>;
}

export function AgentReceiptCard({
  disabled = false,
  onRequestChanges,
  onReviewed,
  receipt,
  reviewed = false,
}: AgentReceiptCardProps) {
  const { engineering } = receipt;
  const lights = receipt.lights.reduce<
    Array<AgentReceiptModel["lights"][number] & { occurrence: number }>
  >((result, light) => {
    const occurrence = result.filter(
      (item) => item.label === light.label,
    ).length;
    result.push({ ...light, occurrence });
    return result;
  }, []);
  const hasEngineering =
    engineering.prRef !== null ||
    engineering.branch !== null ||
    engineering.filesChanged.length > 0 ||
    engineering.ci.length > 0;

  return (
    <section
      className="max-w-2xl rounded-lg border border-border/70 bg-muted/30 p-3 text-sm"
      data-testid="agent-receipt-card"
    >
      <p className="font-medium text-foreground">{receipt.summary}</p>

      {receipt.lights.length > 0 && (
        <div className="mt-3 grid gap-1">
          {lights.map((light) => (
            <div
              className="flex items-center justify-between gap-3"
              key={`${light.label}-${light.occurrence}`}
            >
              <span className="text-muted-foreground">{light.label}</span>
              <span className="font-medium">{light.status}</span>
            </div>
          ))}
        </div>
      )}

      <p className="mt-3 text-muted-foreground">
        <span className="font-medium text-foreground">Verify:</span>{" "}
        {receipt.verify}
      </p>

      <details className="mt-3 text-xs">
        <summary className="cursor-pointer select-none text-muted-foreground">
          Engineering details
        </summary>
        {hasEngineering ? (
          <dl className="mt-2 grid gap-1">
            {engineering.prRef && (
              <div className="flex gap-2">
                <dt className="font-medium">PR</dt>
                <dd>
                  <ExternalReference value={engineering.prRef} />
                </dd>
              </div>
            )}
            {engineering.branch && (
              <div className="flex gap-2">
                <dt className="font-medium">Branch</dt>
                <dd>{engineering.branch}</dd>
              </div>
            )}
            {engineering.filesChanged.length > 0 && (
              <div className="flex gap-2">
                <dt className="font-medium">Files</dt>
                <dd>{engineering.filesChanged.join(", ")}</dd>
              </div>
            )}
            {engineering.ci.length > 0 && (
              <div className="flex gap-2">
                <dt className="font-medium">CI</dt>
                <dd>
                  <table className="border-collapse">
                    <thead className="sr-only">
                      <tr>
                        <th scope="col">Lane</th>
                        <th scope="col">Status</th>
                      </tr>
                    </thead>
                    <tbody>
                      {engineering.ci.map((check) => (
                        <tr key={`${check.label}-${check.status}`}>
                          <td className="pr-3">{check.label}</td>
                          <td>{check.status || "—"}</td>
                        </tr>
                      ))}
                    </tbody>
                  </table>
                </dd>
              </div>
            )}
          </dl>
        ) : (
          <p className="mt-2 text-muted-foreground">No engineering details.</p>
        )}
      </details>
      {onReviewed || onRequestChanges ? (
        <div className="mt-3 flex flex-wrap items-center gap-2 border-t border-border/60 pt-3">
          {onReviewed ? (
            <button
              className="rounded-md bg-primary px-2.5 py-1 text-xs font-medium text-primary-foreground transition-colors hover:bg-primary/90 disabled:opacity-50"
              data-testid="agent-receipt-reviewed"
              disabled={disabled || reviewed}
              onClick={onReviewed}
              type="button"
            >
              {reviewed ? "Reviewed" : "Mark reviewed"}
            </button>
          ) : null}
          {onRequestChanges ? (
            <button
              className="rounded-md border border-border px-2.5 py-1 text-xs font-medium text-foreground transition-colors hover:bg-accent disabled:opacity-50"
              data-testid="agent-receipt-request-changes"
              disabled={disabled}
              onClick={onRequestChanges}
              type="button"
            >
              Request changes
            </button>
          ) : null}
        </div>
      ) : null}
    </section>
  );
}

import { useState } from "react";

import type { ImetaLookup } from "@/shared/ui/markdown/types";
import { Markdown } from "@/shared/ui/markdown";
import type {
  TimelineMessage,
  TimelineReaction,
} from "@/features/messages/types";
import type { UserProfileLookup } from "@/features/profile/lib/identity";
import { normalizePubkey } from "@/shared/lib/pubkey";
import { resolvePrReferenceHref } from "./AgentReceiptCard";
import type { EvidenceKind } from "@/features/messages/lib/evidenceTag";
import { useEvidenceCrossCheck } from "@/features/messages/lib/useEvidenceCrossCheck";
import {
  EvidenceCrossCheckBadge,
  EvidenceCrossCheckDetail,
} from "./EvidenceCrossCheckBadge";

type EvidenceCardProps = {
  canToggleReactions: boolean;
  currentPubkey?: string;
  imetaByUrl?: ImetaLookup;
  kind: EvidenceKind;
  message: TimelineMessage;
  onReply?: (message: TimelineMessage) => void;
  onToggleReaction?: (emoji: string) => Promise<void>;
  profiles?: UserProfileLookup;
  reactionPending: boolean;
  reactions: readonly TimelineReaction[];
};

function bodyMarkdown(message: TimelineMessage) {
  return <Markdown content={message.body} className="text-sm" />;
}

function reactionIsCurrentUser(
  reactions: readonly TimelineReaction[],
  emoji: string,
) {
  return reactions.some(
    (reaction) =>
      reaction.emoji === emoji && reaction.reactedByCurrentUser === true,
  );
}

function MetricsLayout({ message }: { message: TimelineMessage }) {
  const values = new Map<string, string>();
  for (const match of message.body.matchAll(
    /(before|after|delta)\s*:\s*([^|,\n]+)/gi,
  )) {
    values.set(match[1].toLowerCase(), match[2].trim());
  }
  return (
    <div className="grid gap-2">
      {values.size > 0 ? (
        <>
          <div className="grid grid-cols-3 gap-2 text-xs text-muted-foreground">
            <span>before</span>
            <span>after</span>
            <span>delta</span>
          </div>
          <div className="grid grid-cols-3 gap-2 font-medium">
            <span>{values.get("before") ?? "—"}</span>
            <span>{values.get("after") ?? "—"}</span>
            <span>{values.get("delta") ?? "—"}</span>
          </div>
        </>
      ) : null}
      {bodyMarkdown(message)}
    </div>
  );
}

function TestRunLayout({ message }: { message: TimelineMessage }) {
  const failed = message.body.match(/failed[^|,\n]*/i)?.[0];
  const passed = message.body.match(/passed[^|,\n]*/i)?.[0];
  return (
    <div className="grid gap-2">
      {failed ? (
        <div className="rounded-md border border-destructive/40 bg-destructive/10 p-2">
          <p className="text-xs font-medium text-destructive">Failing</p>
          <Markdown content={failed} className="text-sm" />
        </div>
      ) : null}
      {passed ? (
        <div className="rounded-md border border-emerald-500/40 bg-emerald-500/10 p-2">
          <p className="text-xs font-medium text-emerald-700 dark:text-emerald-300">
            Passing
          </p>
          <Markdown content={passed} className="text-sm" />
        </div>
      ) : null}
      {bodyMarkdown(message)}
    </div>
  );
}

function DiffStatLayout({ message }: { message: TimelineMessage }) {
  const prReference = message.body.match(
    /(?:https?:\/\/\S+|(?:[\w.-]+\/[\w.-]+)?#\d+)/,
  )?.[0];
  const href = prReference ? resolvePrReferenceHref(prReference) : null;
  return (
    <div className="grid gap-2">
      <Markdown content={message.body} className="text-sm" />
      {prReference && href ? (
        <a
          className="text-sm text-primary underline underline-offset-2"
          href={href}
          rel="noreferrer"
          target="_blank"
        >
          {prReference}
        </a>
      ) : null}
    </div>
  );
}

function VisualLayout({
  imetaByUrl,
  message,
}: {
  imetaByUrl?: ImetaLookup;
  message: TimelineMessage;
}) {
  const [failedUrls, setFailedUrls] = useState<Set<string>>(new Set());
  const urls = [...message.body.matchAll(/https?:\/\/\S+/g)].map((match) =>
    match[0].replace(/[),]+$/, ""),
  );
  const entries = urls
    .map((url) => ({ url, entry: imetaByUrl?.get(url) }))
    .filter(({ entry }) => entry);
  if (entries.length === 0) return bodyMarkdown(message);

  return (
    <div className="grid grid-cols-2 gap-2">
      {entries.map(({ url }, index) =>
        failedUrls.has(url) ? (
          <a
            className="rounded-md border border-border p-2 text-sm text-primary underline"
            href={url}
            key={url}
            rel="noreferrer"
            target="_blank"
          >
            {index === 0 ? "Before capture" : "After capture"}
          </a>
        ) : (
          <figure className="grid gap-1" key={url}>
            <img
              alt={index === 0 ? "Before capture" : "After capture"}
              className="max-h-64 w-full rounded-md object-contain"
              onError={() =>
                setFailedUrls((current) => new Set(current).add(url))
              }
              src={url}
            />
            <figcaption className="text-xs text-muted-foreground">
              {index === 0 ? "Before capture" : "After capture"}
            </figcaption>
          </figure>
        ),
      )}
    </div>
  );
}

export function EvidenceCard({
  canToggleReactions,
  currentPubkey,
  imetaByUrl,
  kind,
  message,
  onReply,
  onToggleReaction,
  profiles,
  reactionPending,
  reactions,
}: EvidenceCardProps) {
  const ownerPubkey = message.pubkey
    ? profiles?.[normalizePubkey(message.pubkey)]?.ownerPubkey
    : null;
  const isOwner = Boolean(
    currentPubkey &&
      ownerPubkey &&
      normalizePubkey(currentPubkey) === normalizePubkey(ownerPubkey),
  );
  const showControls = isOwner && canToggleReactions && onToggleReaction;
  const accepted = reactionIsCurrentUser(reactions, "✅");
  const rejected = reactionIsCurrentUser(reactions, "❌");
  const crossCheck = useEvidenceCrossCheck(kind, message);
  const heading =
    kind === "test-run"
      ? "Test run"
      : kind === "before-after-visual"
        ? "Before/after visual"
        : kind === "diff-stat"
          ? "Diff stat"
          : "Metrics";
  const layout =
    kind === "metrics" ? (
      <MetricsLayout message={message} />
    ) : kind === "test-run" ? (
      <TestRunLayout message={message} />
    ) : kind === "diff-stat" ? (
      <DiffStatLayout message={message} />
    ) : (
      <VisualLayout imetaByUrl={imetaByUrl} message={message} />
    );

  return (
    <section
      className="max-w-2xl rounded-lg border border-border/70 bg-muted/30 p-3 text-sm"
      data-testid={`evidence-card-${kind}`}
    >
      <div className="mb-3 flex items-center justify-between gap-2">
        <h3 className="font-medium text-foreground">{heading}</h3>
        <div className="flex min-w-0 items-center gap-2">
          <EvidenceCrossCheckBadge result={crossCheck} />
          {rejected ? (
            <span data-testid="evidence-reaction-rejected">❌ Rejected</span>
          ) : accepted ? (
            <span data-testid="evidence-reaction-accepted">✅ Accepted</span>
          ) : null}
        </div>
      </div>
      <EvidenceCrossCheckDetail result={crossCheck} />
      {layout}
      {showControls ? (
        <div className="mt-3 flex gap-2 border-t border-border/60 pt-3">
          <button
            className="rounded-md bg-primary px-2.5 py-1 text-xs font-medium text-primary-foreground disabled:opacity-50"
            data-testid="evidence-accept"
            disabled={reactionPending || accepted}
            onClick={() => void onToggleReaction("✅")}
            type="button"
          >
            Accept
          </button>
          <button
            className="rounded-md border border-border px-2.5 py-1 text-xs font-medium disabled:opacity-50"
            data-testid="evidence-reject"
            disabled={reactionPending || rejected}
            onClick={async () => {
              await onToggleReaction("❌");
              onReply?.(message);
            }}
            type="button"
          >
            Reject
          </button>
        </div>
      ) : null}
    </section>
  );
}

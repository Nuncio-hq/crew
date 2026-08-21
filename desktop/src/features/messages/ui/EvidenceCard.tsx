import { useState } from "react";

import type { ImetaLookup } from "@/shared/ui/markdown/types";
import { Markdown } from "@/shared/ui/markdown";
import type {
  TimelineMessage,
  TimelineReaction,
} from "@/features/messages/types";
import type { UserProfileLookup } from "@/features/profile/lib/identity";
import { normalizePubkey } from "@/shared/lib/pubkey";
import { parseEntityLink } from "@/shared/lib/entityLink";
import { useOpenEntityLink } from "@/shared/ui/markdown/entityLinks";
import { parseEvidenceClaim } from "@/features/messages/lib/evidenceCrossCheck";
import { splitEvidenceBody } from "@/features/messages/lib/evidenceBodyParts";
import type { EvidenceKind } from "@/features/messages/lib/evidenceTag";
import { useEvidenceCrossCheck } from "@/features/messages/lib/useEvidenceCrossCheck";
import { cn } from "@/shared/lib/cn";
import { DiffStatSummary, TestRunSummary } from "./ci/CiPresentation";
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

function reactionIsCurrentUser(
  reactions: readonly TimelineReaction[],
  emoji: string,
) {
  return reactions.some(
    (reaction) =>
      reaction.emoji === emoji && reaction.reactedByCurrentUser === true,
  );
}

function EvidenceNarrative({ content }: { content: string }) {
  if (!content.trim()) return null;
  return <Markdown content={content} className="text-sm" />;
}

function EvidenceLinks({
  links,
}: {
  links: ReturnType<typeof splitEvidenceBody>["links"];
}) {
  const openEntity = useOpenEntityLink();
  if (links.length === 0) return null;

  return (
    <div className="flex flex-wrap gap-2" data-testid="evidence-links">
      {links.map((link) => {
        if (link.kind === "buzz-pr") {
          const parsed = parseEntityLink(link.href);
          return (
            <button
              className="rounded-md border border-border/70 bg-background/50 px-2.5 py-1 text-2xs font-medium text-foreground transition-colors hover:bg-muted"
              data-testid="evidence-link-buzz-pr"
              key={link.href}
              onClick={() => {
                if (parsed.ok) openEntity(parsed.value);
              }}
              title={link.href}
              type="button"
            >
              {link.label}
            </button>
          );
        }
        return (
          <a
            className="rounded-md border border-border/70 bg-background/50 px-2.5 py-1 text-2xs font-medium text-foreground no-underline transition-colors hover:bg-muted"
            data-testid={
              link.kind === "github-pr"
                ? "evidence-link-github-pr"
                : "evidence-link-other"
            }
            href={link.href}
            key={link.href}
            rel="noreferrer"
            target="_blank"
            title={link.href}
          >
            {link.label}
          </a>
        );
      })}
    </div>
  );
}

function MetricsLayout({ message }: { message: TimelineMessage }) {
  const values = new Map<string, string>();
  for (const match of message.body.matchAll(
    /(before|after|delta)\s*:\s*([^|,\n]+)/gi,
  )) {
    values.set(match[1].toLowerCase(), match[2].trim());
  }
  const parts = splitEvidenceBody(message.body);
  const narrative =
    values.size > 0
      ? parts.narrative
          .replace(/(?:^|\|\s*)(?:before|after|delta)\s*:\s*[^|\n]+/gi, "")
          .replace(/^\s*\|\s*|\s*\|\s*$/g, "")
          .trim()
      : parts.narrative || message.body;
  return (
    <div className="grid gap-3">
      {values.size > 0 ? (
        <div className="grid grid-cols-1 gap-2 rounded-md border border-border/60 bg-background/40 px-2.5 py-2 [@container(min-width:21.25rem)]:grid-cols-3">
          {(["before", "after", "delta"] as const).map((key) => (
            <div className="min-w-0" key={key}>
              <p className="text-2xs text-muted-foreground">{key}</p>
              <p className="truncate font-medium tabular-nums">
                {values.get(key) ?? "—"}
              </p>
            </div>
          ))}
        </div>
      ) : null}
      <EvidenceNarrative content={narrative} />
      <EvidenceLinks links={parts.links} />
    </div>
  );
}

function TestRunLayout({ message }: { message: TimelineMessage }) {
  const claim = parseEvidenceClaim("test-run", message.body);
  const parts = splitEvidenceBody(message.body);
  return (
    <div className="grid gap-3">
      {claim && claim.kind === "test-run" ? (
        <TestRunSummary
          failed={claim.failed}
          passed={claim.passed}
          skipped={claim.skipped}
        />
      ) : null}
      <EvidenceNarrative content={parts.narrative} />
      <EvidenceLinks links={parts.links} />
      {/* Fallback when the body has no Tests: line — show raw markdown once. */}
      {!claim && parts.narrative.length === 0 ? (
        <Markdown content={message.body} className="text-sm" />
      ) : null}
    </div>
  );
}

function DiffStatLayout({ message }: { message: TimelineMessage }) {
  const claim = parseEvidenceClaim("diff-stat", message.body);
  const parts = splitEvidenceBody(message.body);
  return (
    <div className="grid gap-3">
      {claim && claim.kind === "diff-stat" ? (
        <div
          className="rounded-md border border-border/60 bg-background/40 px-2.5 py-2"
          data-testid="evidence-diff-stat"
        >
          <p className="mb-1 text-2xs font-medium text-muted-foreground">
            Diff claimed
          </p>
          <DiffStatSummary
            additions={claim.additions}
            deletions={claim.deletions}
            files={claim.files}
          />
        </div>
      ) : null}
      <EvidenceNarrative content={parts.narrative} />
      <EvidenceLinks links={parts.links} />
      {!claim && parts.narrative.length === 0 && parts.links.length === 0 ? (
        <Markdown content={message.body} className="text-sm" />
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
  if (entries.length === 0) {
    return <Markdown content={message.body} className="text-sm" />;
  }

  return (
    <div className="grid grid-cols-1 gap-2 [@container(min-width:21.25rem)]:grid-cols-2">
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
      className="@container min-w-0 max-w-2xl rounded-lg border border-border/70 bg-muted/30 p-3 text-sm"
      data-testid={`evidence-card-${kind}`}
    >
      <div className="mb-3 flex min-w-0 items-center justify-between gap-2">
        <h3 className="min-w-0 truncate font-medium text-foreground">
          {heading}
        </h3>
        <div className="flex min-w-0 items-center gap-2">
          <EvidenceCrossCheckBadge result={crossCheck} />
          {rejected ? (
            <span
              className="text-2xs font-medium text-destructive"
              data-testid="evidence-reaction-rejected"
            >
              Changes requested
            </span>
          ) : accepted ? (
            <span
              className="text-2xs font-medium text-success"
              data-testid="evidence-reaction-accepted"
            >
              Evidence accepted
            </span>
          ) : null}
        </div>
      </div>
      <EvidenceCrossCheckDetail result={crossCheck} />
      {layout}
      {showControls ? (
        <div className="mt-3 border-t border-border/60 pt-3">
          <p className="mb-2 text-2xs text-muted-foreground">
            Review this evidence claim — does not merge or approve the PR.
          </p>
          <div className="flex flex-wrap gap-2">
            <button
              className={cn(
                "rounded-md bg-primary px-2.5 py-1 text-xs font-medium text-primary-foreground disabled:opacity-50",
              )}
              data-testid="evidence-accept"
              disabled={reactionPending || accepted}
              onClick={() => void onToggleReaction("✅")}
              type="button"
            >
              Accept evidence
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
              Request changes
            </button>
          </div>
        </div>
      ) : null}
    </section>
  );
}

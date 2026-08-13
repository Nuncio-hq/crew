import * as React from "react";

import type { ProjectThreadBadge } from "@/features/messages/lib/projectThreadBadge";
import { projectThreadStatusClassName } from "@/features/messages/lib/projectThreadGitHubStatus";
import { cn } from "@/shared/lib/cn";

export function ProjectThreadBadgeChips({
  badge,
}: {
  badge: ProjectThreadBadge;
}) {
  const branchText = badge.label ?? badge.shortBranch;
  return (
    <span
      className="inline-flex min-w-0 flex-wrap items-center text-2xs font-normal text-muted-foreground/70"
      data-testid="project-thread-badge-chips"
    >
      <span className="mx-1 text-muted-foreground/50">·</span>
      <span
        className={cn(
          "inline-flex max-w-60 min-w-0 items-center gap-0.5",
          badge.mono && "font-mono",
        )}
        title={badge.branch}
      >
        {badge.glyph === "📁" && badge.href ? (
          <a
            className="inline-flex min-w-0 items-center gap-0.5 text-inherit no-underline"
            data-testid="project-thread-cowork-chip"
            href={badge.href}
            onClick={(event) => event.stopPropagation()}
          >
            <span aria-hidden="true">{badge.glyph}</span>
            <span className="truncate">{badge.label ?? "cowork"}</span>
          </a>
        ) : (
          <>
            <span aria-hidden="true">{badge.glyph}</span>
            <span
              className={cn(
                "truncate",
                "[@container(max-width:659.9px)]:max-w-24",
                "[@container(max-width:519.9px)]:max-w-16",
                "[@container(max-width:419.9px)]:hidden",
              )}
              data-testid="project-thread-badge-branch-text"
            >
              {branchText}
            </span>
          </>
        )}
      </span>
      {badge.pullRequests.map((pr) => (
        <React.Fragment key={pr.number}>
          <span className="mx-1 text-muted-foreground/50">·</span>
          <span
            className={cn(
              "inline-flex items-center gap-0.5 tabular-nums",
              projectThreadStatusClassName(pr.tone),
            )}
            title={pr.title}
          >
            #{pr.number}
            {pr.checkGlyph ? (
              <span aria-hidden="true">{pr.checkGlyph}</span>
            ) : null}
          </span>
        </React.Fragment>
      ))}
      {badge.overflow > 0 ? (
        <>
          <span className="mx-1 text-muted-foreground/50">·</span>
          <span className="tabular-nums">+{badge.overflow}</span>
        </>
      ) : null}
      {badge.openIssues ? (
        <>
          <span className="mx-1 text-muted-foreground/50">·</span>
          <span
            className="inline-flex items-center gap-0.5 tabular-nums text-emerald-600 dark:text-emerald-400"
            data-testid="project-thread-badge-open-issues"
            title={badge.openIssues.title}
          >
            <span aria-hidden="true">◉</span>
            {badge.openIssues.openCount}
          </span>
        </>
      ) : null}
      {badge.diff ? (
        <span
          className="[@container(max-width:659.9px)]:hidden"
          data-testid="project-thread-badge-diff"
        >
          <span className="mx-1 text-muted-foreground/50">·</span>
          <span
            className="tabular-nums"
            title={`+${badge.diff.additions} −${badge.diff.deletions}`}
          >
            <span className="text-emerald-600 dark:text-emerald-400">
              +{badge.diff.additions}
            </span>{" "}
            <span className="text-destructive">−{badge.diff.deletions}</span>
          </span>
        </span>
      ) : null}
    </span>
  );
}

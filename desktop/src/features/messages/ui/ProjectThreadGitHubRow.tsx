import { CircleDot, GitPullRequest, ListChecks } from "lucide-react";

import type { ThreadPullRequest } from "@/shared/api/thread-workspace-types";
import type { ProjectThreadPhaseStates } from "@/features/messages/lib/projectThreadMissionControl";
import { ProjectThreadIntegrationCell } from "./ProjectThreadIntegrationCell";
import type { ProjectThreadDrawer } from "./ProjectThreadIntegrationDrawer";
import {
  ciStatus,
  projectThreadStatusClassName,
  pullRequestStatus,
} from "@/features/messages/lib/projectThreadGitHubStatus";
import { summarizeThreadChecks } from "./ci/checkPresentation";

export function ProjectThreadGitHubRow({
  activeDrawer,
  onToggle,
  phases,
  pullRequest,
}: {
  activeDrawer: ProjectThreadDrawer | null;
  onToggle: (drawer: ProjectThreadDrawer) => void;
  phases: Pick<ProjectThreadPhaseStates, "pr" | "ci">;
  pullRequest: ThreadPullRequest;
}) {
  const issue = pullRequest.closingIssuesReferences[0];
  const checks = summarizeThreadChecks(pullRequest.checks);
  const pullRequestStatusValue = pullRequestStatus(pullRequest);
  const ciStatusValue = ciStatus(pullRequest.checks);
  const ciDetail =
    pullRequest.checks.length === 0
      ? "No checks yet"
      : [
          checks.failed > 0 ? `${checks.failed} failed` : null,
          checks.running > 0 ? `${checks.running} running` : null,
          `${checks.passed} passed`,
        ]
          .filter(Boolean)
          .join(" · ");
  return (
    <div className="grid grid-cols-3 overflow-hidden rounded-xl border border-border/60 bg-muted/20 [&>*:not(:last-child)]:border-r">
      <ProjectThreadIntegrationCell
        active={activeDrawer === "issue"}
        detail={issue?.title ?? "No linked issue"}
        icon={<CircleDot className="h-3.5 w-3.5" />}
        label="Issue"
        onClick={() => onToggle("issue")}
        title={issue ? `#${issue.number}` : "Unlinked"}
      />
      <ProjectThreadIntegrationCell
        active={activeDrawer === "pr"}
        detail={`${pullRequest.state.toLowerCase()} · ${pullRequest.reviewDecision || "review pending"}`}
        icon={<GitPullRequest className="h-3.5 w-3.5" />}
        label="Pull request"
        onClick={() => onToggle("pr")}
        phase={phases.pr}
        statusClassName={projectThreadStatusClassName(
          pullRequestStatusValue.tone,
        )}
        title={`${pullRequestStatusValue.label} · PR #${pullRequest.number}`}
      />
      <ProjectThreadIntegrationCell
        active={activeDrawer === "ci"}
        detail={ciDetail}
        icon={<ListChecks className="h-3.5 w-3.5" />}
        label="GitHub CI"
        onClick={() => onToggle("ci")}
        phase={phases.ci}
        statusClassName={projectThreadStatusClassName(ciStatusValue.tone)}
        title={ciStatusValue.label}
      />
    </div>
  );
}

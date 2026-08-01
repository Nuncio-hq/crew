import { CircleDot, GitPullRequest, ListChecks } from "lucide-react";

import type { ThreadPullRequest } from "@/shared/api/thread-workspace-types";
import { ProjectThreadIntegrationCell } from "./ProjectThreadIntegrationCell";
import type { ProjectThreadDrawer } from "./ProjectThreadIntegrationDrawer";
import {
  ciStatus,
  projectThreadStatusClassName,
  pullRequestStatus,
} from "./projectThreadGitHubStatus";

function ciSummary(states: readonly string[]) {
  let passed = 0;
  let failed = 0;
  for (const value of states) {
    const state = value.toUpperCase();
    if (["SUCCESS", "NEUTRAL", "SKIPPED"].includes(state)) passed += 1;
    else if (["FAILURE", "ERROR", "CANCELLED", "TIMED_OUT"].includes(state))
      failed += 1;
  }
  return { failed, passed, running: states.length - passed - failed };
}

export function ProjectThreadGitHubRow({
  activeDrawer,
  onToggle,
  pullRequest,
}: {
  activeDrawer: ProjectThreadDrawer | null;
  onToggle: (drawer: ProjectThreadDrawer) => void;
  pullRequest: ThreadPullRequest;
}) {
  const issue = pullRequest.closingIssuesReferences[0];
  const checks = ciSummary(pullRequest.checks.map((check) => check.state));
  const pullRequestStatusValue = pullRequestStatus(pullRequest);
  const ciStatusValue = ciStatus(pullRequest.checks);
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
        statusClassName={projectThreadStatusClassName(
          pullRequestStatusValue.tone,
        )}
        title={`${pullRequestStatusValue.label} · PR #${pullRequest.number}`}
      />
      <ProjectThreadIntegrationCell
        active={activeDrawer === "ci"}
        detail={`${checks.passed}/${pullRequest.checks.length} passed · ${checks.running} running`}
        icon={<ListChecks className="h-3.5 w-3.5" />}
        label="CI"
        onClick={() => onToggle("ci")}
        statusClassName={projectThreadStatusClassName(ciStatusValue.tone)}
        title={ciStatusValue.label}
      />
    </div>
  );
}

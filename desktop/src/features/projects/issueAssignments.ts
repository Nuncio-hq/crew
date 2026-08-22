import { useMutation, useQueryClient } from "@tanstack/react-query";
import * as React from "react";

import {
  signProjectIssueAssignment,
  signProjectIssueUnassignment,
} from "@/shared/api/projectGit";
import { relayClient } from "@/shared/api/relayClient";
import { signRelayEvent } from "@/shared/api/tauri";
import { KIND_TEXT_NOTE } from "@/shared/constants/kinds";
import type { Repository } from "./hooks";
import {
  ISSUE_ASSIGNMENT_LABEL,
  ISSUE_UNASSIGNMENT_LABEL,
  nextProjectIssueCommentCreatedAt,
  type ProjectIssue,
} from "./projectIssues.mjs";

type IssueAssignmentOperation = "assign" | "unassign";
type IssueAssignmentMutationInput = {
  assignees: string[];
  assigneeLabel: string;
  issue: ProjectIssue;
  signerPubkey: string;
  signAsManagedOwner: boolean;
};

async function writeProjectIssueAssignment({
  assignees,
  assigneeLabel,
  issue,
  operation,
  project,
  signerPubkey,
  signAsManagedOwner,
}: IssueAssignmentMutationInput & {
  operation: IssueAssignmentOperation;
  project: Repository;
}): Promise<void> {
  const normalizedAssignees = [
    ...new Set(assignees.map((pubkey) => pubkey.toLowerCase())),
  ];
  if (normalizedAssignees.length === 0) {
    throw new Error("Select at least one assignee.");
  }
  const normalizedSigner = signerPubkey.toLowerCase();
  const authorized =
    signAsManagedOwner ||
    normalizedSigner === issue.author.toLowerCase() ||
    normalizedSigner === project.owner.toLowerCase() ||
    (normalizedAssignees.length === 1 &&
      normalizedAssignees[0] === normalizedSigner);
  if (!authorized) {
    throw new Error("You can only assign or unassign yourself.");
  }
  const createdAt = nextProjectIssueCommentCreatedAt(
    issue,
    Math.floor(Date.now() / 1_000),
    signAsManagedOwner ? project.owner : normalizedSigner,
  );
  const assignment = operation === "assign";
  const normalizedLabel = assigneeLabel.trim();
  if (
    normalizedLabel.length === 0 ||
    Array.from(normalizedLabel).length > 128
  ) {
    throw new Error("Assignee label must be between 1 and 128 characters.");
  }
  if (signAsManagedOwner) {
    const signManagedOperation = assignment
      ? signProjectIssueAssignment
      : signProjectIssueUnassignment;
    await signManagedOperation({
      targetOwner: project.owner,
      repoAddress: project.repoAddress,
      issueId: issue.id,
      assignees: normalizedAssignees,
      assigneeLabel: normalizedLabel,
      createdAt,
    });
    return;
  }
  const prior =
    normalizedAssignees.length === 1 &&
    normalizedAssignees[0] === normalizedSigner
      ? issue.assigneeOperationHeads[normalizedSigner]
      : undefined;
  const event = await signRelayEvent({
    kind: KIND_TEXT_NOTE,
    content: assignment
      ? `Assigned this issue to ${normalizedLabel}`
      : `Unassigned ${normalizedLabel} from this issue`,
    createdAt,
    tags: [
      ["e", issue.id, "", "root"],
      ["a", project.repoAddress],
      ...normalizedAssignees.map((pubkey) => ["p", pubkey]),
      ["t", assignment ? ISSUE_ASSIGNMENT_LABEL : ISSUE_UNASSIGNMENT_LABEL],
      ...(prior ? [["prior", prior]] : []),
    ],
  });
  await relayClient.publishEvent(
    event,
    `Timed out ${operation}ing issue.`,
    `Failed to ${operation} issue.`,
  );
}

function useProjectIssueAssignmentMutation(
  project: Repository,
  operation: IssueAssignmentOperation,
) {
  const queryClient = useQueryClient();
  const invalidate = React.useCallback(() => {
    void queryClient.invalidateQueries({
      queryKey: ["project", project.id, "issues"],
    });
    void queryClient.invalidateQueries({
      queryKey: ["projects", "work-items"],
    });
  }, [project.id, queryClient]);
  return useMutation({
    mutationFn: (input: IssueAssignmentMutationInput) =>
      writeProjectIssueAssignment({
        ...input,
        operation,
        project,
      }),
    onSuccess: invalidate,
  });
}

export function useAssignProjectIssueMutation(project: Repository) {
  return useProjectIssueAssignmentMutation(project, "assign");
}

export function useUnassignProjectIssueMutation(project: Repository) {
  return useProjectIssueAssignmentMutation(project, "unassign");
}

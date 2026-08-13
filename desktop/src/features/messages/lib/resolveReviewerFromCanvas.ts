import type {
  CanvasResponse,
  CanvasRoleAssignment,
  CanvasRoutingPreset,
} from "@/shared/api/types";

const CODE_REVIEW_WORK_TYPE = "code-review";
const REVIEWER_LABEL = "reviewer";

export type ReviewerResolution =
  | {
      status: "held";
      pubkey: string;
      roleLabel: string;
      source: "routing" | "label" | "manual";
    }
  | { status: "unheld"; roleLabel: string };

export type ReviewerManualStore = {
  channelPubkey?: string | null;
  threadPubkey?: string | null;
};

export function resolveReviewerFromCanvas(
  canvas: Pick<CanvasResponse, "routing" | "assignments"> | null | undefined,
  manual: ReviewerManualStore = {},
): ReviewerResolution {
  if (manual.threadPubkey) {
    return {
      status: "held",
      pubkey: manual.threadPubkey,
      roleLabel: "Reviewer",
      source: "manual",
    };
  }
  const routing = canvas?.routing ?? [];
  const preset = routing.find(
    (entry) => entry.workType.trim().toLowerCase() === CODE_REVIEW_WORK_TYPE,
  );
  const routed = firstHolder(preset);
  if (routed) {
    return {
      status: "held",
      pubkey: routed,
      roleLabel: preset?.roleLabel || "Reviewer",
      source: "routing",
    };
  }
  const labeled = firstLabeledReviewer(canvas?.assignments ?? []);
  if (labeled) {
    return {
      status: "held",
      pubkey: labeled,
      roleLabel: "Reviewer",
      source: "label",
    };
  }
  if (manual.channelPubkey) {
    return {
      status: "held",
      pubkey: manual.channelPubkey,
      roleLabel: "Reviewer",
      source: "manual",
    };
  }
  return { status: "unheld", roleLabel: preset?.roleLabel || "Reviewer" };
}

function firstHolder(preset: CanvasRoutingPreset | undefined): string | null {
  const holder = preset?.holders.find((value) => value.trim().length > 0);
  return holder ?? null;
}

function firstLabeledReviewer(
  assignments: readonly CanvasRoleAssignment[],
): string | null {
  const match = assignments.find(
    (entry) => entry.roleLabel.trim().toLowerCase() === REVIEWER_LABEL,
  );
  return match?.agentPubkey ?? null;
}

export function reviewerStorageKey(channelId: string): string {
  return `crew.forge.reviewer.channel.${channelId}`;
}

export function reviewerThreadStorageKey(rootEventId: string): string {
  return `crew.forge.reviewer.thread.${rootEventId}`;
}

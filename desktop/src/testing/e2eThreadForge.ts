/**
 * In-memory forge PR hub snapshot for the e2e mock bridge.
 *
 * Seed via `window.__BUZZ_E2E_FORGE_PR__` in addInitScript, or
 * `__BUZZ_E2E_SET_FORGE_PR_DETAIL__` after boot.
 */

import type {
  ForgeActionResult,
  ForgeAvailability,
  ForgeCheckLogResult,
  ForgeDetailResult,
  ForgeDiffResult,
  ForgePullRequestDetail,
} from "@/shared/api/threadForgeTypes";

const FORGE_COMMANDS = new Set([
  "get_thread_forge_pr_detail",
  "get_thread_forge_pr_diff",
  "get_forge_check_log_tail",
  "rerun_forge_checks",
  "comment_forge_pr",
  "review_forge_pr",
  "merge_forge_pr",
  "set_forge_file_viewed",
  "resolve_forge_pr_by_url",
  "create_forge_pr",
]);

export function isForgeCommand(command: string): boolean {
  return FORGE_COMMANDS.has(command);
}

export type E2eForgeSnapshot = {
  availability: ForgeAvailability;
  rateLimitedUntil: string | null;
  detail: ForgePullRequestDetail | null;
  message: string | null;
  diffSource: "worktree" | "api";
};

export function defaultForgePullRequestDetail(): ForgePullRequestDetail {
  return {
    id: "PR_e2e_hub",
    number: 193,
    title: "GitHub PR hub in thread focus",
    body: "Ship the two-tier review surface.",
    url: "https://github.com/Nuncio-hq/crew/pull/193",
    state: "open",
    isDraft: false,
    headRefName: "cursor/github-pr-hub-thread-focus-fa2a",
    baseRefName: "main",
    additions: 42,
    deletions: 7,
    changedFiles: 2,
    reviewDecision: "changes-requested",
    mergeStateStatus: "CLEAN",
    author: { login: "founder" },
    comments: [
      {
        id: "IC_1",
        author: { login: "alice" },
        body: "Looks close.",
        createdAt: "2026-08-13T10:00:00Z",
        url: "https://github.com/Nuncio-hq/crew/pull/193#issuecomment-1",
      },
    ],
    reviews: [
      {
        id: "PRR_1",
        author: { login: "bob" },
        body: "Please fix the checks tab.",
        state: "CHANGES_REQUESTED",
        submittedAt: "2026-08-13T10:05:00Z",
        url: "https://github.com/Nuncio-hq/crew/pull/193#pullrequestreview-1",
      },
    ],
    reviewThreads: [
      {
        id: "PRT_1",
        isResolved: false,
        isOutdated: false,
        path: "desktop/src/hub.ts",
        line: 12,
        comments: [
          {
            id: "RTC_1",
            author: { login: "bob" },
            body: "This path is forge-specific.",
            createdAt: "2026-08-13T10:06:00Z",
            url: "https://github.com/Nuncio-hq/crew/pull/193#discussion-1",
          },
        ],
      },
    ],
    commits: [
      {
        oid: "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
        messageHeadline: "feat(desktop): GitHub PR hub",
        committedAt: "2026-08-13T09:00:00Z",
        additions: 40,
        deletions: 5,
        authorName: "Founder",
        authorEmail: "founder@example.com",
      },
    ],
    files: [
      {
        path: "desktop/src/hub.ts",
        additions: 40,
        deletions: 5,
        viewedState: "unviewed",
      },
      {
        path: "docs/crew/DECISIONS.md",
        additions: 2,
        deletions: 2,
        viewedState: "viewed",
      },
    ],
    checks: [
      {
        name: "NuncioCrew Gate",
        status: "COMPLETED",
        conclusion: "success",
        url: "https://github.com/Nuncio-hq/crew/actions/runs/1",
        workflow: "NuncioCrew Gate",
        runId: 1001,
        startedAt: "2026-08-13T09:10:00Z",
        completedAt: "2026-08-13T09:12:00Z",
      },
      {
        name: "Desktop Fast",
        status: "COMPLETED",
        conclusion: "failure",
        url: "https://github.com/Nuncio-hq/crew/actions/runs/2",
        workflow: "Desktop Fast",
        runId: 1002,
        startedAt: "2026-08-13T09:10:00Z",
        completedAt: "2026-08-13T09:11:00Z",
      },
      {
        name: "lint",
        status: "IN_PROGRESS",
        conclusion: "pending",
        url: null,
        workflow: "Desktop Fast",
        runId: 1002,
        startedAt: "2026-08-13T09:11:00Z",
        completedAt: null,
      },
    ],
    mergeStrategies: ["merge", "squash", "rebase"],
    filesTruncated: false,
    commitsTruncated: false,
    checksTruncated: false,
  };
}

export function defaultForgeSnapshot(): E2eForgeSnapshot {
  return {
    availability: "available",
    rateLimitedUntil: null,
    detail: defaultForgePullRequestDetail(),
    message: null,
    diffSource: "worktree",
  };
}

function readSnapshot(): E2eForgeSnapshot {
  const seeded = (
    globalThis as typeof globalThis & {
      __BUZZ_E2E_FORGE_PR__?: E2eForgeSnapshot;
    }
  ).__BUZZ_E2E_FORGE_PR__;
  return seeded ?? defaultForgeSnapshot();
}

function writeSnapshot(next: E2eForgeSnapshot): E2eForgeSnapshot {
  (
    globalThis as typeof globalThis & {
      __BUZZ_E2E_FORGE_PR__?: E2eForgeSnapshot;
    }
  ).__BUZZ_E2E_FORGE_PR__ = next;
  return next;
}

function detailResult(snapshot: E2eForgeSnapshot): ForgeDetailResult {
  return {
    availability: snapshot.availability,
    rateLimitedUntil: snapshot.rateLimitedUntil,
    detail: snapshot.availability === "available" ? snapshot.detail : null,
    message: snapshot.message,
  };
}

function diffResult(snapshot: E2eForgeSnapshot): ForgeDiffResult {
  const files = snapshot.detail?.files ?? [];
  return {
    availability: snapshot.availability,
    rateLimitedUntil: snapshot.rateLimitedUntil,
    diff:
      snapshot.availability === "available"
        ? {
            files: files.map((file) => ({
              path: file.path,
              additions: file.additions,
              deletions: file.deletions,
              patch: `diff --git a/${file.path} b/${file.path}\n--- a/${file.path}\n+++ b/${file.path}\n@@ -1 +1 @@\n-old\n+new\n`,
              truncated: false,
            })),
            additions: snapshot.detail?.additions ?? 0,
            deletions: snapshot.detail?.deletions ?? 0,
            source: snapshot.diffSource,
          }
        : null,
    message: snapshot.message,
  };
}

export function handleForgeCommand(command: string, payload: unknown): unknown {
  const snapshot = readSnapshot();
  const args = (payload ?? {}) as Record<string, unknown>;

  switch (command) {
    case "get_thread_forge_pr_detail":
    case "resolve_forge_pr_by_url":
      return detailResult(snapshot);
    case "get_thread_forge_pr_diff":
      return diffResult(snapshot);
    case "get_forge_check_log_tail": {
      const log: ForgeCheckLogResult = {
        availability: snapshot.availability,
        rateLimitedUntil: snapshot.rateLimitedUntil,
        tails:
          snapshot.availability === "available"
            ? [
                {
                  job: "Desktop Fast",
                  step: "test",
                  lines: [
                    "Run pnpm test",
                    "##[error] Assertion failed: hub tab missing",
                    "Process completed with exit code 1",
                  ],
                  truncated: false,
                },
              ]
            : [],
        message: snapshot.message,
      };
      return log;
    }
    case "rerun_forge_checks":
    case "comment_forge_pr":
    case "review_forge_pr":
    case "merge_forge_pr":
    case "set_forge_file_viewed":
    case "create_forge_pr": {
      if (snapshot.availability !== "available") {
        throw new Error(snapshot.message ?? "Forge CLI is unavailable.");
      }
      if (command === "set_forge_file_viewed" && snapshot.detail) {
        const path = String(args.path ?? "");
        const viewed = Boolean(args.viewed);
        snapshot.detail.files = snapshot.detail.files.map((file) =>
          file.path === path
            ? { ...file, viewedState: viewed ? "viewed" : "unviewed" }
            : file,
        );
        writeSnapshot(snapshot);
      }
      if (command === "comment_forge_pr" && snapshot.detail) {
        snapshot.detail.comments = [
          ...snapshot.detail.comments,
          {
            id: `IC_${snapshot.detail.comments.length + 1}`,
            author: { login: "you" },
            body: String(args.body ?? ""),
            createdAt: new Date().toISOString(),
            url: "https://github.com/Nuncio-hq/crew/pull/193#issuecomment-new",
          },
        ];
        writeSnapshot(snapshot);
      }
      const result: ForgeActionResult = {
        ok: true,
        message: `${command} ok`,
      };
      return result;
    }
    default:
      throw new Error(`Unhandled forge command: ${command}`);
  }
}

export function setE2eForgeSnapshot(
  patch: Partial<E2eForgeSnapshot>,
): E2eForgeSnapshot {
  return writeSnapshot({ ...readSnapshot(), ...patch });
}

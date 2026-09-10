import type { RelayEvent } from "@/shared/api/types";
import type { OwnerOperationScope } from "@/shared/api/ownerOperations";
import {
  KIND_LONG_FORM,
  KIND_REPO_STATE,
  KIND_REPO_WIKI_PAGE,
} from "@/shared/constants/kinds";

export const WIKI_TOC_SLUG = "_toc";
export const COMPANY_PROPOSAL_PREFIX = "_proposal/";

export type WikiCadence = "manual" | "on-push" | "daily" | "weekly";

export type WikiTocPage = {
  slug: string;
  title: string;
};

export type WikiTocSection = {
  id: string;
  title: string;
  pages: WikiTocPage[];
};

export type WikiToc = {
  event: RelayEvent;
  repoD: string;
  owner: string;
  commit: string;
  branch: string;
  cadence: WikiCadence;
  sections: WikiTocSection[];
  generatedAt: number;
};

export type WikiFreshness = "never" | "fresh" | "stale" | "unknown";

export type WikiPage = {
  event: RelayEvent;
  repoD: string;
  slug: string;
  title: string;
  section: string;
  commit: string;
  language: string;
  sourceFiles: string[];
  content: string;
};

export type CompanyWikiPage = {
  event: RelayEvent;
  slug: string;
  title: string;
  content: string;
  proposal: boolean;
  engramSlug: string | null;
  status: string | null;
};

export type WikiJobState = {
  repoKey: string;
  /** Native owner/community/generation fence for renderer job projections. */
  scope?: OwnerOperationScope;
  /** Durable native journal identity used by explicit recovery controls. */
  operationId?: string;
  operationRevision?: number;
  nativeStatus?: string;
  reconciled?: boolean;
  attempts?: number;
  retryAt?: number;
  headAttempted?: boolean;
  cancelRequested?: boolean;
  reconcileOnly?: boolean;
  /** Exact dependency ID for the typed immutable-retired proof. */
  retiredDependencyId?: string;
  snapshotId?: string;
  sourceRevision?: string;
  cadence?: WikiCadence;
  status: "idle" | "generating" | "failed";
  done: number;
  total: number;
  error: string | null;
  costNote: string | null;
};

/**
 * The one explicit recovery action a durable Wiki row may offer.
 *
 * `resume` and `retry` both reach native through the same `retry` command
 * (`explicitRetry: true`); they differ only in what the user is being told,
 * and native decides what that explicit action is allowed to revoke.
 */
export type WikiRecoveryAffordance = "none" | "retry" | "resume" | "regenerate";

/**
 * Pick the accurate action for a durable recovery row.
 *
 * Order matters and mirrors the native rules:
 * - a reconciled row is terminal, so it offers no publication action at all;
 * - a typed immutable-retired proof is Regenerate-only — offering "Retry" or
 *   "Resume" there would promise a resubmission native will never perform;
 * - an unresolved cancelled / read-only row needs its cancellation revoked
 *   before anything can be submitted, which is Resume, not an ordinary Retry;
 * - anything else unresolved is an ordinary bounded retry.
 */
export function wikiRecoveryAffordance(
  job: WikiJobState | undefined,
): WikiRecoveryAffordance {
  if (!job?.operationId || job.reconciled) return "none";
  if (job.retiredDependencyId) return "regenerate";
  if (job.cancelRequested || job.reconcileOnly) return "resume";
  return "retry";
}

/** Button text for an affordance; `null` when no action should be offered. */
export function wikiRecoveryActionLabel(
  affordance: WikiRecoveryAffordance,
): string | null {
  switch (affordance) {
    case "retry":
      return "Retry publication";
    case "resume":
      return "Resume publication";
    case "regenerate":
      return "Regenerate from source";
    default:
      return null;
  }
}

/**
 * Whether Cancel should be offered for a durable row.
 *
 * Cancel stops automatic attempts, so it only applies where attempts are still
 * possible: a terminal row, a non-durable row and an already cancelled row
 * have nothing left to stop. A typed retired dependency is excluded by the
 * same affordance rule — it is already read-only and Regenerate-only, and
 * native refuses a Cancel there rather than overwrite the retirement proof the
 * successor flow depends on.
 */
export function wikiCanCancelRecovery(job: WikiJobState | undefined): boolean {
  const affordance = wikiRecoveryAffordance(job);
  return (
    (affordance === "retry" || affordance === "resume") && !job?.cancelRequested
  );
}

function tagValue(event: RelayEvent, name: string): string | undefined {
  return event.tags.find((tag) => tag[0] === name)?.[1];
}

function tagValues(event: RelayEvent, name: string): string[] {
  return event.tags.filter((tag) => tag[0] === name).map((tag) => tag[1] ?? "");
}

export function parseWikiDTag(
  d: string | undefined,
): { repoD: string; slug: string } | null {
  if (!d) return null;
  const slash = d.lastIndexOf("/");
  if (slash <= 0) return null;
  return { repoD: d.slice(0, slash), slug: d.slice(slash + 1) };
}

export function parseWikiToc(event: RelayEvent): WikiToc | null {
  if (event.kind !== KIND_REPO_WIKI_PAGE) return null;
  const parsed = parseWikiDTag(tagValue(event, "d"));
  if (!parsed || parsed.slug !== WIKI_TOC_SLUG) return null;
  const a = tagValue(event, "a") ?? "";
  const owner = a.split(":")[1] ?? event.pubkey;
  let sections: WikiTocSection[] = [];
  try {
    const body = JSON.parse(event.content) as {
      sections?: WikiTocSection[];
      generated_at?: number;
    };
    sections = body.sections ?? [];
  } catch {
    sections = [];
  }
  const cadenceRaw = tagValue(event, "cadence") ?? "manual";
  const cadence: WikiCadence =
    cadenceRaw === "on-push" ||
    cadenceRaw === "daily" ||
    cadenceRaw === "weekly"
      ? cadenceRaw
      : "manual";
  return {
    event,
    repoD: parsed.repoD,
    owner: owner.toLowerCase(),
    commit: tagValue(event, "commit") ?? "",
    branch: tagValue(event, "branch") ?? "main",
    cadence,
    sections,
    generatedAt: event.created_at,
  };
}

export function parseWikiPage(event: RelayEvent): WikiPage | null {
  if (event.kind !== KIND_REPO_WIKI_PAGE) return null;
  const parsed = parseWikiDTag(tagValue(event, "d"));
  if (!parsed || parsed.slug === WIKI_TOC_SLUG) return null;
  return {
    event,
    repoD: parsed.repoD,
    slug: parsed.slug,
    title: tagValue(event, "title") ?? parsed.slug,
    section: tagValue(event, "section") ?? "overview",
    commit: tagValue(event, "commit") ?? "",
    language: tagValue(event, "language") ?? "en",
    sourceFiles: tagValues(event, "source").filter(Boolean),
    content: event.content,
  };
}

export function parseCompanyWikiPage(
  event: RelayEvent,
): CompanyWikiPage | null {
  if (event.kind !== KIND_LONG_FORM) return null;
  const slug = tagValue(event, "d") ?? "";
  if (!slug) return null;
  const proposal = slug.startsWith(COMPANY_PROPOSAL_PREFIX);
  return {
    event,
    slug,
    title: tagValue(event, "title") ?? slug,
    content: event.content,
    proposal,
    engramSlug: tagValue(event, "crew-engram-slug") ?? null,
    status: tagValue(event, "crew-wiki-status") ?? null,
  };
}

export function defaultBranchCommit(stateEvent: RelayEvent | undefined): {
  branch: string;
  commit: string;
} | null {
  if (!stateEvent || stateEvent.kind !== KIND_REPO_STATE) return null;

  let head: string | null = null;
  const refs = new Map<string, string>();
  for (const tag of stateEvent.tags) {
    const name = tag[0];
    if (!name) continue;
    if (name === "HEAD") {
      if (head !== null || tag.length !== 2 || !validDefaultHead(tag[1])) {
        return null;
      }
      head = tag[1];
      continue;
    }
    if (name.startsWith("refs/heads/") || name.startsWith("refs/tags/")) {
      if (
        tag.length !== 2 ||
        !validRefName(name) ||
        !validRefOid(tag[1]) ||
        refs.has(name)
      ) {
        return null;
      }
      refs.set(name, tag[1]);
    }
  }

  if (!head) return null;
  const branch = head.slice("ref: refs/heads/".length);
  const ref = `refs/heads/${branch}`;
  const commit = refs.get(ref);
  return commit ? { branch, commit } : null;
}

export function wikiFreshness(
  toc: WikiToc | null,
  state: RelayEvent | undefined,
): WikiFreshness {
  if (!toc) return "never";
  const tip = defaultBranchCommit(state);
  if (!tip?.commit) return "unknown";
  if (toc.commit && tip.commit && toc.commit !== tip.commit) return "stale";
  return "fresh";
}

function validDefaultHead(value: string | undefined): value is string {
  return (
    typeof value === "string" &&
    value.startsWith("ref: refs/heads/") &&
    validRefTail(value.slice("ref: refs/heads/".length))
  );
}

function validRefName(value: string): boolean {
  const prefix = value.startsWith("refs/heads/")
    ? "refs/heads/"
    : value.startsWith("refs/tags/")
      ? "refs/tags/"
      : null;
  return (
    prefix !== null &&
    !value.endsWith("/") &&
    !value.includes("//") &&
    !value.includes("..") &&
    validRefTail(value.slice(prefix.length))
  );
}

function validRefTail(value: string): boolean {
  return (
    value.length > 0 &&
    !value.startsWith(".") &&
    !value.endsWith(".") &&
    /^[A-Za-z0-9/_.-]+$/.test(value)
  );
}

function validRefOid(value: string | undefined): value is string {
  return (
    typeof value === "string" &&
    (value.length === 40 || value.length === 64) &&
    /^[0-9a-f]+$/.test(value)
  );
}

export function repoKey(owner: string, repoD: string): string {
  return `${owner.toLowerCase()}:${repoD}`;
}

/** Resolve jobs only for the exact repository owner and identifier. */
export function jobForRepo(
  jobs: Map<string, WikiJobState>,
  owner: string,
  repoD: string,
): WikiJobState | undefined {
  return jobs.get(repoKey(owner, repoD));
}

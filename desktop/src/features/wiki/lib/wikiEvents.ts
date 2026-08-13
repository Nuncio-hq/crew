import type { RelayEvent } from "@/shared/api/types";
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
  status: "idle" | "generating" | "failed";
  done: number;
  total: number;
  error: string | null;
  costNote: string | null;
};

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
  const head = tagValue(stateEvent, "HEAD") ?? "ref: refs/heads/main";
  const branch = head.replace(/^ref: refs\/heads\//, "");
  const commit =
    tagValue(stateEvent, `refs/heads/${branch}`) ??
    stateEvent.tags.find((tag) => tag[0]?.startsWith("refs/heads/"))?.[1] ??
    "";
  return { branch, commit };
}

export function wikiFreshness(
  toc: WikiToc | null,
  state: RelayEvent | undefined,
): "never" | "fresh" | "stale" {
  if (!toc) return "never";
  const tip = defaultBranchCommit(state);
  if (!tip?.commit) return "fresh";
  if (toc.commit && tip.commit && toc.commit !== tip.commit) return "stale";
  return "fresh";
}

export function repoKey(owner: string, repoD: string): string {
  return `${owner.toLowerCase()}:${repoD}`;
}

/**
 * `buzz://` deep links for Buzz-hosted git entities, mirroring
 * `features/messages/lib/messageLink.ts` for `buzz://message`.
 *
 * Formats:
 *   buzz://file?owner=<owner-pubkey>&d=<repo-dtag>&path=<p>&lines=<a>-<b>
 *   buzz://pr?id=<event-id>&owner=<owner-pubkey>&d=<repo-dtag>
 *   buzz://issue?id=<event-id>&owner=<owner-pubkey>&d=<repo-dtag>
 *
 * `owner` + `d` identify the NIP-34 repository coordinate
 * (`30617:<owner>:<d>`); `id` is the kind 1618 / 1621 event id. The CLI
 * builder in `crates/buzz-cli/src/links.rs` emits the same format — the two
 * must stay compatible (see the golden-format tests on both sides).
 */

const ENTITY_LINK_SCHEME = "buzz:";

export type ParsedEntityLink =
  | { type: "pr"; id: string; owner: string; dtag: string }
  | { type: "issue"; id: string; owner: string; dtag: string }
  | {
      type: "repo";
      owner: string;
      dtag: string;
      tab?: EntityLinkTab;
      commitHash?: string;
    }
  | { type: "project"; owner: string; dtag: string; tab?: EntityLinkTab }
  | {
      type: "file";
      owner: string;
      dtag: string;
      path: string;
      startLine: number;
      endLine: number;
    };

export type EntityLinkParseResult =
  | { ok: true; value: ParsedEntityLink }
  | { ok: false; reason: string };

const HEX64_RE = /^[a-fA-F0-9]{64}$/;
const GIT_OBJECT_ID_RE = /^(?:[a-fA-F0-9]{40}|[a-fA-F0-9]{64})$/;
const DTAG_RE = /^[a-zA-Z0-9._-]{1,64}$/;

export const ENTITY_LINK_TABS = [
  "files",
  "commits",
  "issues",
  "prs",
  "contributors",
  "channels",
] as const;

export type EntityLinkTab = (typeof ENTITY_LINK_TABS)[number];

export function isEntityLinkTab(value: unknown): value is EntityLinkTab {
  return (
    typeof value === "string" &&
    (ENTITY_LINK_TABS as readonly string[]).includes(value)
  );
}

function isValidDtag(dtag: string): boolean {
  return DTAG_RE.test(dtag) && !dtag.startsWith(".") && !dtag.includes("..");
}

function checkCoordinate(owner: string, dtag: string): void {
  if (!HEX64_RE.test(owner)) {
    throw new Error("entityLink: owner must be a 64-char hex pubkey");
  }
  if (!isValidDtag(dtag)) {
    throw new Error("entityLink: invalid repository d-tag");
  }
}

function checkEventId(id: string): void {
  if (!HEX64_RE.test(id)) {
    throw new Error("entityLink: id must be a 64-char hex event id");
  }
}

/** Build a `buzz://repo` link for a repository announcement (kind 30617). */
function tabSuffix(tab: EntityLinkTab | undefined): string {
  if (tab === undefined) return "";
  if (!isEntityLinkTab(tab)) {
    throw new Error("entityLink: unknown workspace tab");
  }
  return `&tab=${tab}`;
}

export function buildRepoLink(input: {
  owner: string;
  dtag: string;
  tab?: EntityLinkTab;
}): string {
  checkCoordinate(input.owner, input.dtag);
  return `buzz://repo?owner=${input.owner.toLowerCase()}&d=${input.dtag}${tabSuffix(input.tab)}`;
}

/** Build a `buzz://project` link for a project announcement (kind 30621). */
export function buildProjectLink(input: {
  owner: string;
  dtag: string;
  tab?: EntityLinkTab;
}): string {
  checkCoordinate(input.owner, input.dtag);
  return `buzz://project?owner=${input.owner.toLowerCase()}&d=${input.dtag}${tabSuffix(input.tab)}`;
}

/** Build a link to a specific commit in a repository. */
export function buildCommitLink(input: {
  commitHash: string;
  owner: string;
  dtag: string;
}): string {
  checkCoordinate(input.owner, input.dtag);
  if (!GIT_OBJECT_ID_RE.test(input.commitHash)) {
    throw new Error("entityLink: commit must be a 40- or 64-char hex hash");
  }
  return `buzz://repo?owner=${input.owner.toLowerCase()}&d=${input.dtag}&tab=commits&commit=${input.commitHash.toLowerCase()}`;
}

/** Build a `buzz://pr` link for a pull request event (kind 1618). */
export function buildPullRequestLink(input: {
  id: string;
  owner: string;
  dtag: string;
}): string {
  checkEventId(input.id);
  checkCoordinate(input.owner, input.dtag);
  return `buzz://pr?id=${input.id.toLowerCase()}&owner=${input.owner.toLowerCase()}&d=${input.dtag}`;
}

/** Build a `buzz://file` link for a repository path and line range. */
export function buildFileLink(input: {
  owner: string;
  dtag: string;
  path: string;
  lines: string;
}): string {
  checkCoordinate(input.owner, input.dtag);
  checkFilePath(input.path);
  checkLines(input.lines);
  const params = new URLSearchParams({
    owner: input.owner.toLowerCase(),
    d: input.dtag,
    path: input.path,
    lines: input.lines,
  });
  return `buzz://file?${params.toString()}`;
}

function checkFilePath(path: string): void {
  if (
    !path ||
    path.startsWith("/") ||
    path.includes("..") ||
    path.length > 512
  ) {
    throw new Error("entityLink: invalid file path");
  }
}

function checkLines(lines: string): void {
  if (!/^\d+(-\d+)?$/.test(lines)) {
    throw new Error("entityLink: invalid lines");
  }
}
export function buildIssueLink(input: {
  id: string;
  owner: string;
  dtag: string;
}): string {
  checkEventId(input.id);
  checkCoordinate(input.owner, input.dtag);
  return `buzz://issue?id=${input.id.toLowerCase()}&owner=${input.owner.toLowerCase()}&d=${input.dtag}`;
}

/**
 * Cheap pre-check used by the markdown renderer and preview extraction
 * before parsing. `buzz://message` is intentionally excluded — it has its
 * own pill rendering path.
 */
export function isEntityLink(href: string | undefined | null): boolean {
  if (!href) return false;
  return (
    href.startsWith("buzz://pr?") ||
    href.startsWith("buzz://issue?") ||
    href.startsWith("buzz://repo?") ||
    href.startsWith("buzz://project?") ||
    href.startsWith("buzz://file?")
  );
}

/**
 * Parse a `buzz://pr|issue|repo?…` URL. Returns a discriminated result so
 * callers can fall back to plain-link rendering without throwing. All
 * identifiers are validated; hex values are lowercase-normalized.
 *
 * Strict canonical form to preserve forward-compatibility:
 * - Empty or root path only (no `/extra/segments`)
 * - No fragment
 * - Each required parameter must appear exactly once
 * - Unknown query parameters are rejected (callers that need to add
 *   parameters must version the format or add them to the known-params set)
 *
 * This ensures old clients decline rather than silently misinterpret future
 * extensions (e.g. the reserved `relay=` cross-community field).
 */
export function parseEntityLink(url: string): EntityLinkParseResult {
  let parsed: URL;
  try {
    parsed = new URL(url);
  } catch {
    return { ok: false, reason: "invalid-url" };
  }

  if (parsed.protocol !== ENTITY_LINK_SCHEME) {
    return { ok: false, reason: "wrong-scheme" };
  }

  const host = parsed.hostname;
  if (
    host !== "pr" &&
    host !== "issue" &&
    host !== "repo" &&
    host !== "project" &&
    host !== "file"
  ) {
    return { ok: false, reason: "wrong-host" };
  }

  // Require empty/root path — path segments are reserved for future versioning.
  if (parsed.pathname !== "" && parsed.pathname !== "/") {
    return { ok: false, reason: "unexpected-path" };
  }

  // Reject fragments — not part of the canonical format.
  if (parsed.hash) {
    return { ok: false, reason: "unexpected-fragment" };
  }

  // Validate known params and reject unknown ones, and enforce single-instance.
  const KNOWN_REPO_PARAMS = new Set(["owner", "d", "tab", "commit"]);
  const KNOWN_PROJECT_PARAMS = new Set(["owner", "d", "tab"]);
  const KNOWN_EVENT_PARAMS = new Set(["id", "owner", "d"]);
  const KNOWN_FILE_PARAMS = new Set(["owner", "d", "path", "lines"]);
  const knownParams =
    host === "repo"
      ? KNOWN_REPO_PARAMS
      : host === "project"
        ? KNOWN_PROJECT_PARAMS
        : host === "file"
          ? KNOWN_FILE_PARAMS
          : KNOWN_EVENT_PARAMS;

  for (const key of parsed.searchParams.keys()) {
    if (!knownParams.has(key)) {
      return { ok: false, reason: "unknown-param" };
    }
  }
  for (const key of knownParams) {
    const values = parsed.searchParams.getAll(key);
    if (values.length > 1) {
      return { ok: false, reason: "duplicate-param" };
    }
  }

  const owner = parsed.searchParams.get("owner");
  const dtag = parsed.searchParams.get("d");
  if (!owner || !HEX64_RE.test(owner)) {
    return { ok: false, reason: "invalid-owner" };
  }
  if (!dtag || !isValidDtag(dtag)) {
    return { ok: false, reason: "invalid-dtag" };
  }

  if (host === "repo" || host === "project") {
    const tab = parsed.searchParams.get("tab");
    if (tab !== null && !isEntityLinkTab(tab)) {
      return { ok: false, reason: "invalid-tab" };
    }
    const commitHash =
      host === "repo" ? parsed.searchParams.get("commit") : null;
    if (
      commitHash !== null &&
      (tab !== "commits" || !GIT_OBJECT_ID_RE.test(commitHash))
    ) {
      return { ok: false, reason: "invalid-commit" };
    }
    return {
      ok: true,
      value: {
        type: host,
        owner: owner.toLowerCase(),
        dtag,
        ...(tab !== null && isEntityLinkTab(tab) ? { tab } : {}),
        ...(commitHash !== null
          ? { commitHash: commitHash.toLowerCase() }
          : {}),
      },
    };
  }

  if (host === "file") {
    const path = parsed.searchParams.get("path");
    const lines = parsed.searchParams.get("lines");
    if (!path || path.startsWith("/") || path.includes("..")) {
      return { ok: false, reason: "invalid-path" };
    }
    if (!lines || !/^\d+(-\d+)?$/.test(lines)) {
      return { ok: false, reason: "invalid-lines" };
    }
    const [startRaw, endRaw] = lines.split("-");
    const startLine = Number(startRaw);
    const endLine = Number(endRaw ?? startRaw);
    return {
      ok: true,
      value: {
        type: "file",
        owner: owner.toLowerCase(),
        dtag,
        path,
        startLine,
        endLine,
      },
    };
  }

  const id = parsed.searchParams.get("id");
  if (!id || !HEX64_RE.test(id)) {
    return { ok: false, reason: "invalid-id" };
  }

  return {
    ok: true,
    value: {
      type: host,
      id: id.toLowerCase(),
      owner: owner.toLowerCase(),
      dtag,
    },
  };
}

/**
 * Canonical NIP-34 repository coordinate (`30617:<owner>:<dtag>`) used as the
 * route id for `/projects/$projectId`. Duncan's #4671 branch resolves 30617
 * coordinates regardless of which explicit project contains the repository, so
 * entity links remain stable when a repo's container project changes.
 *
 * Do NOT use the legacy `<owner>:<dtag>` form — it only matched implicit
 * project cards and breaks for repos claimed by an explicit project with a
 * different d-tag.
 */
export function entityLinkProjectRouteId(link: ParsedEntityLink): string {
  const kind = link.type === "project" ? 30621 : 30617;
  return `${kind}:${link.owner}:${link.dtag}`;
}

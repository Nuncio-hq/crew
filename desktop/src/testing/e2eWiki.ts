import type { RelayEvent } from "@/shared/api/types";
import { KIND_LONG_FORM, KIND_REPO_WIKI_PAGE } from "@/shared/constants/kinds";
import type { WikiJobState } from "@/features/wiki/lib/wikiEvents";
import { setWikiJob } from "@/features/wiki/lib/wikiStore";

type MockFilter = {
  kinds?: number[];
  "#d"?: string[];
  authors?: string[];
};

const MOCK_SIG = `mocksig${"0".repeat(121)}`.slice(0, 128);
const MOCK_WIKI_COMMIT = "0123456789abcdef0123456789abcdef01234567";
const MOCK_WIKI_SNAPSHOT = "12345678-1234-4234-9234-123456789abc";
const MOCK_WIKI_SOURCE_PATH =
  "desktop/src/features/projects/ui/ProjectDetailScreen.tsx";
const MOCK_WIKI_SOURCE_CONTENT = [
  "export function ProjectDetailScreen() {",
  '  return <CommunityTabs defaultValue="files" />;',
  "}",
].join("\n");

type WikiSourceReference = [string, string, number, number, number];

let wikiEvents: RelayEvent[] = [];

function mockEventId(seed: string): string {
  const hex = seed
    .replace(/[^0-9a-f]/gi, "a")
    .toLowerCase()
    .padEnd(64, "a");
  return hex.slice(0, 64);
}

export function resetE2eWiki(): void {
  wikiEvents = [];
}

export function setE2eWikiEvents(events: RelayEvent[]): void {
  wikiEvents = events.map((event) => ({
    ...event,
    id: event.id.length === 64 ? event.id : mockEventId(event.id),
    sig: event.sig.startsWith("mocksig") ? event.sig : MOCK_SIG,
  }));
}

export function wikiPageEvent(input: {
  owner: string;
  repoD: string;
  slug: string;
  title: string;
  content: string;
  commit?: string;
  snapshotId?: string;
  sources?: string[];
}): RelayEvent {
  const sources = input.sources ?? [MOCK_WIKI_SOURCE_PATH];
  const sourceReferences: WikiSourceReference[] = sources.map((path) => [
    path,
    "c".repeat(64),
    new TextEncoder().encode(MOCK_WIKI_SOURCE_CONTENT).byteLength,
    1,
    3,
  ]);
  return {
    id: mockEventId(`wiki${input.repoD}${input.slug}`),
    pubkey: input.owner,
    created_at: Math.floor(Date.now() / 1000),
    kind: KIND_REPO_WIKI_PAGE,
    tags: [
      ["d", `${input.repoD}/${input.slug}`],
      ["a", `30617:${input.owner}:${input.repoD}`],
      ["title", input.title],
      ["commit", input.commit ?? "generated"],
      ["section", "overview"],
      ["language", "en"],
      ...(input.snapshotId
        ? [
            ["wiki-version", "1"],
            ["wiki-snapshot", input.snapshotId],
            ["source-kind", "git"],
            ["wiki-slug", input.slug],
            ["wiki-source-files", JSON.stringify(sourceReferences)],
          ]
        : []),
      ...sources.map((path) => ["source", path]),
    ],
    content: input.content,
    sig: MOCK_SIG,
  };
}

export function wikiTocEvent(input: {
  owner: string;
  repoD: string;
  commit?: string;
  cadence?: string;
  snapshotId?: string;
  manifestId?: string;
  manifestDigest?: string;
}): RelayEvent {
  const commit = input.commit ?? MOCK_WIKI_COMMIT;
  return {
    id: mockEventId(`toc${input.repoD}`),
    pubkey: input.owner,
    created_at: Math.floor(Date.now() / 1000),
    kind: KIND_REPO_WIKI_PAGE,
    tags: [
      ["d", `${input.repoD}/_toc`],
      ["a", `30617:${input.owner}:${input.repoD}`],
      ["commit", commit],
      ["branch", "main"],
      ["cadence", input.cadence ?? "manual"],
      ["title", "Wiki"],
      ...(input.snapshotId
        ? [
            ["wiki-version", "1"],
            ["wiki-snapshot", input.snapshotId],
            ["source-kind", "git"],
            ["expected-revision", commit],
            ...(input.manifestId && input.manifestDigest
              ? [["wiki-manifest", input.manifestId, input.manifestDigest]]
              : []),
          ]
        : []),
    ],
    content: JSON.stringify({
      sections: [
        {
          id: "overview",
          title: "Overview",
          pages: [{ slug: "overview", title: "Platform Overview" }],
        },
      ],
    }),
    sig: MOCK_SIG,
  };
}

export function companyWikiEvent(input: {
  pubkey: string;
  slug: string;
  title: string;
  content: string;
  proposal?: boolean;
  engramSlug?: string;
}): RelayEvent {
  const slug = input.proposal ? `_proposal/${input.slug}` : input.slug;
  return {
    id: mockEventId(`company${slug}`),
    pubkey: input.pubkey,
    created_at: Math.floor(Date.now() / 1000),
    kind: KIND_LONG_FORM,
    tags: [
      ["d", slug],
      ["title", input.title],
      ...(input.proposal
        ? [
            ["crew-wiki-proposal", "1"],
            ["crew-wiki-status", "pending"],
          ]
        : []),
      ...(input.engramSlug ? [["crew-engram-slug", input.engramSlug]] : []),
    ],
    content: input.content,
    sig: MOCK_SIG,
  };
}

export function seedGeneratedWiki(
  owner: string,
  repoD: string,
  commit?: string,
  contentSuffix?: string,
): RelayEvent[] {
  const sourcePath = MOCK_WIKI_SOURCE_PATH;
  const sourceReferences: WikiSourceReference[] = [
    [
      sourcePath,
      "c".repeat(64),
      new TextEncoder().encode(MOCK_WIKI_SOURCE_CONTENT).byteLength,
      1,
      3,
    ],
  ];
  const sourceRevision = commit ?? MOCK_WIKI_COMMIT;
  const page = wikiPageEvent({
    owner,
    repoD,
    slug: "overview",
    title: "Platform Overview",
    commit: sourceRevision,
    snapshotId: MOCK_WIKI_SNAPSHOT,
    sources: [sourcePath],
    content: [
      "# Platform Overview",
      "",
      "Generated wiki page for E2E.",
      "",
      "The body search proof lives in this verified snapshot.",
      "",
      "```mermaid",
      "flowchart TD",
      "  A[Repo] --> B[Wiki]",
      "```",
      "",
      "```mermaid",
      "this is not valid mermaid {{{",
      "```",
      "",
      `See [ProjectDetailScreen.tsx#L1-3](buzz://file?owner=${owner}&d=${repoD}&path=${sourcePath}&lines=1-3).`,
      "",
      ...(contentSuffix ? [contentSuffix, ""] : []),
    ].join("\n"),
  });
  const manifestDigest = "a".repeat(64);
  const manifestId = mockEventId(`manifest${repoD}${MOCK_WIKI_SNAPSHOT}`);
  const manifest = {
    id: manifestId,
    pubkey: owner,
    created_at: Math.floor(Date.now() / 1000),
    kind: KIND_REPO_WIKI_PAGE,
    tags: [
      ["d", `${repoD}/m1-${manifestDigest}`],
      ["a", `30617:${owner}:${repoD}`],
      ["wiki-version", "1"],
      ["wiki-snapshot", MOCK_WIKI_SNAPSHOT],
      ["source-kind", "git"],
      ["commit", sourceRevision],
    ],
    content: JSON.stringify([
      1,
      MOCK_WIKI_SNAPSHOT,
      owner,
      repoD,
      `git:${sourceRevision}`,
      null,
      [["overview", "Overview", ["overview"]]],
      [
        [
          "overview",
          "overview",
          page.id,
          manifestDigest,
          "Platform Overview",
          "overview",
          "en",
          sourceReferences,
        ],
      ],
    ]),
    sig: MOCK_SIG,
  } satisfies RelayEvent;
  const events = [
    wikiTocEvent({
      owner,
      repoD,
      commit: sourceRevision,
      snapshotId: MOCK_WIKI_SNAPSHOT,
      manifestId,
      manifestDigest,
    }),
    manifest,
    page,
  ];
  const kept = wikiEvents.filter((event) => {
    const d = event.tags.find((tag) => tag[0] === "d")?.[1] ?? "";
    return !d.startsWith(`${repoD}/`);
  });
  wikiEvents = [...kept, ...events];
  return events;
}

/**
 * Return the same complete/legacy graph shape that the native snapshot read
 * exposes to the renderer. The E2E bridge does not verify cryptographic
 * signatures, but it must still exercise the production snapshot boundary so
 * Wiki library/page smoke coverage remains useful after the canonical read
 * moved out of the ambient relay subscription.
 */
export function readE2eWikiSnapshot(
  owner: string,
  repoD: string,
): {
  state: "complete" | "legacy" | "missing";
  head: RelayEvent | null;
  manifest: RelayEvent | null;
  pages: RelayEvent[];
  error: null;
  repo_state: RelayEvent | null;
} {
  const headD = `${repoD}/_toc`;
  const head = wikiEvents.find(
    (event) =>
      event.kind === KIND_REPO_WIKI_PAGE &&
      event.pubkey.toLowerCase() === owner.toLowerCase() &&
      event.tags.some((tag) => tag[0] === "d" && tag[1] === headD),
  );
  if (!head) {
    return {
      state: "missing",
      head: null,
      manifest: null,
      pages: [],
      error: null,
      repo_state: null,
    };
  }
  const manifestId = head.tags.find((tag) => tag[0] === "wiki-manifest")?.[1];
  const manifest = manifestId
    ? (wikiEvents.find((event) => event.id === manifestId) ?? null)
    : null;
  const pages = wikiEvents.filter(
    (event) =>
      event.kind === KIND_REPO_WIKI_PAGE &&
      event.pubkey.toLowerCase() === owner.toLowerCase() &&
      event.tags.some(
        (tag) => tag[0] === "d" && tag[1]?.startsWith(`${repoD}/`),
      ) &&
      !event.tags.some((tag) => tag[0] === "d" && tag[1] === headD) &&
      event.id !== manifest?.id,
  );
  return {
    state: manifest ? "complete" : "legacy",
    head,
    manifest,
    pages,
    error: null,
    repo_state: null,
  };
}

/** Return repository coordinates that currently expose authenticated source references. */
export function wikiSourceCoordinates(owner: string): string[] {
  const coordinates = new Set<string>();
  for (const event of wikiEvents) {
    if (
      event.kind !== KIND_REPO_WIKI_PAGE ||
      event.pubkey.toLowerCase() !== owner.toLowerCase() ||
      !event.tags.some((tag) => tag[0] === "wiki-source-files")
    ) {
      continue;
    }
    const d = event.tags.find((tag) => tag[0] === "d")?.[1] ?? "";
    const slash = d.lastIndexOf("/");
    if (slash > 0) {
      coordinates.add(`30617:${owner.toLowerCase()}:${d.slice(0, slash)}`);
    }
  }
  return [...coordinates];
}

/** Resolve one source reference using the same wire shape as native source access. */
export function readE2eWikiSource(
  page: RelayEvent,
  referenceIndex: number,
): { content: string; startLine: number; endLine: number } | null {
  const tag = page.tags.find(
    (candidate) => candidate[0] === "wiki-source-files",
  );
  if (!tag?.[1]) return null;
  let references: unknown;
  try {
    references = JSON.parse(tag[1]);
  } catch {
    return null;
  }
  if (!Array.isArray(references)) return null;
  const reference = references[referenceIndex];
  if (
    !Array.isArray(reference) ||
    reference.length !== 5 ||
    typeof reference[0] !== "string" ||
    typeof reference[3] !== "number" ||
    typeof reference[4] !== "number"
  ) {
    return null;
  }
  if (reference[0] !== MOCK_WIKI_SOURCE_PATH) return null;
  return {
    content: MOCK_WIKI_SOURCE_CONTENT,
    startLine: reference[3],
    endLine: reference[4],
  };
}

/** Return only body hits from the page ids and v1 coordinate supplied by native search. */
export function searchE2eWikiEvents(input: {
  coordinate: string;
  snapshotId: string;
  pageIds: string[];
  query: string;
}): { events: RelayEvent[]; truncated: boolean } {
  const [, owner = "", repoD = ""] = input.coordinate.split(":");
  const query = input.query.trim().toLocaleLowerCase();
  if (!owner || !repoD || !query) return { events: [], truncated: false };
  const pageIds = new Set(input.pageIds);
  const matches = wikiEvents.filter((event) => {
    const d = event.tags.find((tag) => tag[0] === "d")?.[1] ?? "";
    return (
      event.kind === KIND_REPO_WIKI_PAGE &&
      pageIds.has(event.id) &&
      event.pubkey.toLowerCase() === owner.toLowerCase() &&
      event.tags.some((tag) => tag[0] === "a" && tag[1] === input.coordinate) &&
      event.tags.some(
        (tag) => tag[0] === "wiki-snapshot" && tag[1] === input.snapshotId,
      ) &&
      d.startsWith(`${repoD}/`) &&
      !d.endsWith("/_toc") &&
      event.content.toLocaleLowerCase().includes(query)
    );
  });
  return {
    events: matches.slice(0, 51),
    truncated: matches.length > 51,
  };
}

export function seedCompanyWiki(
  pubkey: string,
  options?: { proposal?: boolean },
): void {
  const events = [
    companyWikiEvent({
      pubkey,
      slug: "welcome",
      title: "Welcome",
      content: "# Welcome\n\nCompany wiki page.",
    }),
  ];
  if (options?.proposal) {
    events.push(
      companyWikiEvent({
        pubkey,
        slug: "engram-note",
        title: "Promoted engram",
        content: "Draft from buzz mem.",
        proposal: true,
        engramSlug: "weekly-retro",
      }),
    );
  }
  wikiEvents = [
    ...wikiEvents.filter((event) => event.kind !== KIND_LONG_FORM),
    ...events,
  ];
}

export function acceptPublishedWikiEvent(event: RelayEvent): boolean {
  if (event.kind !== KIND_REPO_WIKI_PAGE && event.kind !== KIND_LONG_FORM) {
    return false;
  }
  const dTag = event.tags.find((tag) => tag[0] === "d")?.[1];
  if (dTag) {
    wikiEvents = wikiEvents.filter((existing) => {
      const existingD = existing.tags.find((tag) => tag[0] === "d")?.[1];
      return !(existing.kind === event.kind && existingD === dTag);
    });
  }
  wikiEvents.push({
    ...event,
    id: event.id.length === 64 ? event.id : mockEventId(event.id),
    sig: event.sig || MOCK_SIG,
  });
  return true;
}

export function filterE2eWikiEvents(filter: MockFilter): RelayEvent[] {
  const kinds = filter.kinds ?? [];
  const wantsWiki = kinds.includes(KIND_REPO_WIKI_PAGE);
  const wantsNotes = kinds.includes(KIND_LONG_FORM);
  if (!wantsWiki && !wantsNotes) return [];
  return wikiEvents.filter((event) => {
    if (!kinds.includes(event.kind)) return false;
    const dTags = filter["#d"];
    if (dTags) {
      const d = event.tags.find((tag) => tag[0] === "d")?.[1];
      if (!d || !dTags.includes(d)) return false;
    }
    const authors = filter.authors?.map((author) => author.toLowerCase());
    if (authors && !authors.includes(event.pubkey.toLowerCase())) return false;
    return true;
  });
}

export function isWikiKind(kind: number): boolean {
  return kind === KIND_REPO_WIKI_PAGE || kind === KIND_LONG_FORM;
}

export function isWikiCommand(command: string): boolean {
  return command === "wiki_generate";
}

export function handleWikiCommand(command: string, payload: unknown): unknown {
  if (command !== "wiki_generate") return null;
  const input = (payload ?? {}) as { owner?: string; repoD?: string };
  const owner = input.owner ?? "";
  const repoD = input.repoD ?? "buzz";
  seedGeneratedWiki(owner, repoD);
  return { accepted: true, pages: 1, commit: "generated" };
}

export function applyE2eWikiJob(job: WikiJobState): void {
  setWikiJob(job);
}

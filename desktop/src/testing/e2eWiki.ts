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
  sources?: string[];
}): RelayEvent {
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
      ...(
        input.sources ?? [
          "desktop/src/features/projects/ui/ProjectDetailScreen.tsx",
        ]
      ).map((path) => ["source", path]),
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
}): RelayEvent {
  return {
    id: mockEventId(`toc${input.repoD}`),
    pubkey: input.owner,
    created_at: Math.floor(Date.now() / 1000),
    kind: KIND_REPO_WIKI_PAGE,
    tags: [
      ["d", `${input.repoD}/_toc`],
      ["a", `30617:${input.owner}:${input.repoD}`],
      ["commit", input.commit ?? "0123456789abcdef0123456789abcdef01234567"],
      ["branch", "main"],
      ["cadence", input.cadence ?? "manual"],
      ["title", "Wiki"],
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
): RelayEvent[] {
  const filePath = "desktop/src/features/projects/ui/ProjectDetailScreen.tsx";
  const events = [
    wikiTocEvent({ owner, repoD, commit }),
    wikiPageEvent({
      owner,
      repoD,
      slug: "overview",
      title: "Platform Overview",
      commit,
      sources: [filePath],
      content: [
        "# Platform Overview",
        "",
        "Generated wiki page for E2E.",
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
        `See [ProjectDetailScreen.tsx#L1-3](buzz://file?owner=${owner}&d=${repoD}&path=${filePath}&lines=1-3).`,
        "",
      ].join("\n"),
    }),
  ];
  const kept = wikiEvents.filter((event) => {
    const d = event.tags.find((tag) => tag[0] === "d")?.[1] ?? "";
    return !d.startsWith(`${repoD}/`);
  });
  wikiEvents = [...kept, ...events];
  return events;
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

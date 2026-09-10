import { createHash } from "node:crypto";
import { finalizeEvent, getPublicKey } from "nostr-tools/pure";

const key = new Uint8Array(32).fill(1);
const owner = getPublicKey(key);
export const repoD = "Repo.demo";
const uuid = "12345678-1234-4234-9234-123456789abc";
const revision = `git:${"a".repeat(40)}`;
export const digest = (value) =>
  createHash("sha256").update(JSON.stringify(value)).digest("hex");
const signed = (content, tags) =>
  finalizeEvent({ kind: 30623, created_at: 10, content, tags }, key);
const baseTags = (slug, sourceRevision = revision) => [
  ["d", `${repoD}/${slug}`],
  ["a", `30617:${owner}:${repoD}`],
  ["wiki-version", "1"],
  ["wiki-snapshot", uuid],
  ["source-kind", sourceRevision.startsWith("folder:") ? "folder" : "git"],
  [
    "commit",
    sourceRevision.startsWith("folder:")
      ? sourceRevision
      : sourceRevision.slice(4),
  ],
];

export function fixture(changeEnvelope = (value) => value) {
  const refs = [
    [
      "nguồn/file.rs",
      createHash("sha256").update("source\n").digest("hex"),
      7,
      1,
      1,
    ],
  ];
  const envelope = changeEnvelope([
    1,
    uuid,
    owner,
    repoD,
    revision,
    "intro",
    "Giới thiệu",
    "overview",
    "vi",
    refs,
    "# Nội dung\n\nExact source.\n",
  ]);
  const hash = digest(envelope);
  const page = signed(envelope[10], [
    ...baseTags(`p1-${hash}`, envelope[4]),
    ["wiki-slug", "intro"],
    ["title", envelope[6]],
    ["section", envelope[7]],
    ["language", envelope[8]],
    ["wiki-source-files", JSON.stringify(envelope[9])],
    ...envelope[9].map((ref) => ["source", ref[0]]),
  ]);
  const manifestBody = [
    1,
    uuid,
    owner,
    repoD,
    envelope[4],
    null,
    [["overview", "Overview", ["intro"]]],
    [
      [
        "intro",
        `p1-${hash}`,
        page.id,
        hash,
        envelope[6],
        envelope[7],
        envelope[8],
        envelope[9],
      ],
    ],
  ];
  const manifestHash = digest(manifestBody);
  const manifest = signed(
    JSON.stringify(manifestBody),
    baseTags(`m1-${manifestHash}`, envelope[4]),
  );
  const head = signed(
    JSON.stringify({
      sections: [
        {
          id: "overview",
          title: "Overview",
          pages: [{ slug: `p1-${hash}`, title: envelope[6] }],
        },
      ],
    }),
    [
      ...baseTags("_toc", envelope[4]),
      ["wiki-manifest", manifest.id, manifestHash],
      ["expected-revision", "absent"],
    ],
  );
  return { owner, repoD, head, manifest, pages: [page] };
}

export function resign(event) {
  return signed(event.content, event.tags);
}

/** Re-sign all later members after an adversarial validly signed page mutation. */
export function repoint(input) {
  const body = JSON.parse(input.manifest.content);
  body[7][0][2] = input.pages[0].id;
  input.manifest.content = JSON.stringify(body);
  const hash = digest(body);
  input.manifest.tags.find((tag) => tag[0] === "d")[1] =
    `${input.repoD}/m1-${hash}`;
  input.manifest = resign(input.manifest);
  const reference = input.head.tags.find((tag) => tag[0] === "wiki-manifest");
  reference[1] = input.manifest.id;
  reference[2] = hash;
  input.head = resign(input.head);
  return input;
}

/** Re-sign a mutated manifest and its head reference, keeping malformed data explicit. */
export function rewriteIndex(input, change, serialize = JSON.stringify) {
  const body = change(JSON.parse(input.manifest.content));
  const hash = digest(body);
  input.manifest.content = serialize(body);
  input.manifest.tags.find((tag) => tag[0] === "d")[1] =
    `${input.repoD}/m1-${hash}`;
  input.manifest = resign(input.manifest);
  const reference = input.head.tags.find((tag) => tag[0] === "wiki-manifest");
  reference[1] = input.manifest.id;
  reference[2] = hash;
  input.head = resign(input.head);
  return input;
}

/** Build enough valid signed members to exercise the production query batch limit. */
export function multiFixture(count) {
  const input = fixture();
  const manifest = JSON.parse(input.manifest.content);
  const first = manifest[7][0];
  const refs = [];
  const pages = [];
  for (let i = 0; i < count; i += 1) {
    const logical = `page-${i}`;
    const envelope = [
      1,
      manifest[1],
      manifest[2],
      manifest[3],
      manifest[4],
      logical,
      first[4],
      first[5],
      first[6],
      first[7],
      input.pages[0].content,
    ];
    const hash = digest(envelope);
    const tags = input.pages[0].tags.map((tag) =>
      tag[0] === "d"
        ? ["d", `${repoD}/p1-${hash}`]
        : tag[0] === "wiki-slug"
          ? ["wiki-slug", logical]
          : [...tag],
    );
    const page = signed(envelope[10], tags);
    pages.push(page);
    refs.push([logical, `p1-${hash}`, page.id, hash, ...first.slice(4)]);
  }
  input.pages = pages;
  rewriteIndex(input, (body) => {
    body[6][0][2] = refs.map((ref) => ref[0]);
    body[7] = refs;
    return body;
  });
  input.head.content = JSON.stringify({
    sections: [
      {
        id: "overview",
        title: "Overview",
        pages: refs.map((ref) => ({ slug: ref[1], title: ref[4] })),
      },
    ],
  });
  input.head = resign(input.head);
  return input;
}

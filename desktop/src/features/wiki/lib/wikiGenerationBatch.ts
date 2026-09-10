/** Reject a malformed worker response before signing any event. */
export function validateWikiGenerationBatch(
  value: unknown,
  input: { owner: string; repoD: string },
): void {
  function fail(): never {
    throw new Error("Wiki generation returned an invalid or incomplete batch.");
  }
  const object = (item: unknown): Record<string, unknown> => {
    if (!item || typeof item !== "object" || Array.isArray(item)) fail();
    return item as Record<string, unknown>;
  };
  const batch = object(value);
  const drafts = batch.drafts;
  if (
    batch.accepted !== true ||
    !Array.isArray(drafts) ||
    drafts.length === 0 ||
    drafts.length > 256 ||
    batch.pages !== drafts.length ||
    typeof batch.commit !== "string" ||
    !/^(?:[a-f0-9]{40}|[a-f0-9]{64})$/.test(batch.commit) ||
    typeof batch.tocContent !== "string"
  )
    fail();

  const maxContentBytes = 192 * 1024;
  const encoder = new TextEncoder();
  const tocBytes = encoder.encode(batch.tocContent).length;
  if (tocBytes > maxContentBytes) fail();

  const coordinate = `30617:${input.owner}:${input.repoD}`;
  const validateTags = (tags: unknown, slug: string) => {
    if (
      !Array.isArray(tags) ||
      !tags.every(
        (tag) =>
          Array.isArray(tag) && tag.every((part) => typeof part === "string"),
      )
    )
      fail();
    for (const [name, expected] of [
      ["d", `${input.repoD}/${slug}`],
      ["a", coordinate],
      ["commit", batch.commit],
    ]) {
      const matches = tags.filter((tag) => tag[0] === name);
      if (matches.length !== 1 || matches[0][1] !== expected) fail();
    }
  };
  validateTags(batch.tocTags, "_toc");
  let manifest: Record<string, unknown>;
  try {
    manifest = object(JSON.parse(batch.tocContent));
  } catch {
    fail();
  }
  if (!Array.isArray(manifest.sections)) fail();
  const slugs = new Set<string>();
  for (const rawSection of manifest.sections) {
    const section = object(rawSection);
    if (!Array.isArray(section.pages)) fail();
    for (const rawPage of section.pages) {
      const page = object(rawPage);
      if (
        typeof page.slug !== "string" ||
        !/^[a-z0-9](?:[a-z0-9-]{0,78}[a-z0-9])?$/.test(page.slug) ||
        slugs.has(page.slug)
      )
        fail();
      slugs.add(page.slug);
    }
  }
  if (slugs.size !== drafts.length) fail();
  let bytes = tocBytes;
  for (const rawDraft of drafts) {
    const draft = object(rawDraft);
    if (
      typeof draft.slug !== "string" ||
      !slugs.delete(draft.slug) ||
      typeof draft.content !== "string" ||
      !draft.content.trim()
    )
      fail();
    const size = encoder.encode(draft.content).length;
    if (size > maxContentBytes) fail();
    bytes += size;
    validateTags(draft.tags, draft.slug);
  }
  if (bytes > 32 * 1024 * 1024) fail();
}

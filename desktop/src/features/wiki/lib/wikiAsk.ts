export type AskMode = "auto" | "qa" | "plan";

export type WikiAskHit = {
  title: string;
  excerpt: string;
  href?: string;
};

export function ask(input: {
  question: string;
  mode: AskMode;
  hits: WikiAskHit[];
  repoD?: string;
}): {
  mode: AskMode;
  markdown: string;
  citations: Array<{ label: string; href: string }>;
  threadDraft: string | null;
} {
  const q = input.question.toLowerCase();
  const resolved: AskMode =
    input.mode === "auto"
      ? q.includes("plan") || q.startsWith("how should")
        ? "plan"
        : "qa"
      : input.mode;
  const citations = input.hits
    .filter((hit) => hit.href)
    .map((hit) => ({ label: hit.title, href: hit.href ?? "" }));
  if (resolved === "plan") {
    const markdown = `# Plan\n\n${input.question}\n\n1. Read the wiki page and cited files.\n2. Implement the change with tests.\n`;
    return {
      mode: "plan",
      markdown,
      citations,
      threadDraft: `${markdown}\n${citations.map((c) => `- ${c.label}`).join("\n")}`,
    };
  }
  return {
    mode: "qa",
    markdown: input.hits[0]?.excerpt ?? "No wiki hits yet.",
    citations,
    threadDraft: null,
  };
}

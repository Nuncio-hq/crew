export type AskMode = "auto" | "qa" | "plan";

/**
 * Ask stays visibly unavailable until #365 supplies a certified private
 * runtime and #364/#362 supply a validated, coherent Wiki source snapshot.
 * Keep this reason user-facing so a disabled composer does not look broken or
 * imply that a question was answered or saved.
 */
export const WIKI_ASK_UNAVAILABLE_REASON =
  "Private Ask is not available yet. You can still read Wiki pages and search sources. No answer, citation, draft, channel post, or navigation was created.";

export type WikiAskHit = {
  title: string;
  excerpt: string;
  href?: string;
};

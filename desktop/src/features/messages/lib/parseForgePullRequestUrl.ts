export type ForgePullRequestRef = {
  owner: string;
  name: string;
  number: number;
};

export function parseRepoAddress(
  address: string,
): { owner: string; name: string } | null {
  const [owner, name] = address.trim().split("/");
  if (!owner || !name || address.includes("://")) return null;
  return { owner, name: name.replace(/\.git$/, "") };
}

/** Parse owner/name#N or https://host/owner/name/pull/N (and GitLab MR URLs). */
export function parseForgePullRequestUrl(
  input: string,
): ForgePullRequestRef | null {
  const trimmed = input.trim();
  const hash = trimmed.match(/^([^/\s]+)\/([^#\s]+)#(\d+)$/);
  if (hash) {
    return {
      owner: hash[1],
      name: hash[2].replace(/\.git$/, ""),
      number: Number(hash[3]),
    };
  }
  try {
    const url = new URL(trimmed);
    const segments = url.pathname.split("/").filter(Boolean);
    const [owner, name, marker, maybeNumber] = segments;
    if (!owner || !name) return null;
    if (marker === "pull" || marker === "pulls") {
      const number = Number((maybeNumber ?? "").split("/")[0]);
      if (!Number.isInteger(number) || number <= 0) return null;
      return { owner, name: name.replace(/\.git$/, ""), number };
    }
    if (marker === "-" && maybeNumber === "merge_requests") {
      const number = Number((segments[4] ?? "").split("/")[0]);
      if (!Number.isInteger(number) || number <= 0) return null;
      return { owner, name: name.replace(/\.git$/, ""), number };
    }
  } catch {
    return null;
  }
  return null;
}

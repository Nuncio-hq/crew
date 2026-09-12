import type { ParsedEntityLink } from "@/shared/lib/entityLink";

import { useOpenEntityLink } from "./entityLinks";

/** Resolve the shared entity opener, allowing scoped documents to override it. */
export function useEntityLinkHandler(
  override?: (link: ParsedEntityLink, trigger?: HTMLElement | null) => void,
): (link: ParsedEntityLink, trigger?: HTMLElement | null) => void {
  const defaultOpenEntityLink = useOpenEntityLink();
  return override ?? defaultOpenEntityLink;
}

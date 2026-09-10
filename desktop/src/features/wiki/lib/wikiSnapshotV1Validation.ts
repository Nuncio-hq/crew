import { sha256 } from "@noble/hashes/sha2.js";
import { bytesToHex } from "@noble/hashes/utils.js";
import { verifyEvent } from "nostr-tools/pure";
import type { RelayEvent } from "@/shared/api/types";

const encoder = new TextEncoder();
export const HEX64 = /^[0-9a-f]{64}$/;
export const UUID4 =
  /^[0-9a-f]{8}-[0-9a-f]{4}-4[0-9a-f]{3}-[89ab][0-9a-f]{3}-[0-9a-f]{12}$/;
export const REVISION =
  /^(?:git:(?:[0-9a-f]{40}|[0-9a-f]{64})|folder:[0-9a-f]{64})$/;
export const SLUG = /^[a-z0-9](?:[a-z0-9-]{0,78}[a-z0-9])?$/;

/** Reject strings which cannot round-trip through Rust JSON and PostgreSQL. */
export function safeText(value: unknown): value is string {
  if (typeof value !== "string" || value.includes("\0")) return false;
  for (let i = 0; i < value.length; i += 1) {
    const unit = value.charCodeAt(i);
    if (unit >= 0xd800 && unit <= 0xdbff) {
      const next = value.charCodeAt(++i);
      if (!(next >= 0xdc00 && next <= 0xdfff)) return false;
    } else if (unit >= 0xdc00 && unit <= 0xdfff) return false;
  }
  return true;
}

/** Metadata emptiness uses explicit ASCII whitespace, never Unicode trim tables. */
export function metadata(value: unknown): value is string {
  if (!safeText(value)) return false;
  return Array.from(value).some((character) => {
    const code = character.charCodeAt(0);
    return code !== 32 && (code < 9 || code > 13);
  });
}

/** Existing repository identifier grammar, without case normalization. */
export function validRepoD(value: string): boolean {
  return (
    /^[A-Za-z0-9_-][A-Za-z0-9._-]{0,63}$/.test(value) && !value.includes("..")
  );
}

/** Canonical nonnegative safe JSON integer. */
export function integer(value: unknown): value is number {
  return (
    typeof value === "number" &&
    Number.isSafeInteger(value) &&
    value >= 0 &&
    !Object.is(value, -0)
  );
}

/** Hash the fixed-order tuple encoding. Call only after structural validation. */
export function tupleDigest(value: unknown): string {
  return bytesToHex(sha256(encoder.encode(JSON.stringify(value))));
}

/** A duplicate or extra tag component is invalid, never first-match authority. */
export function exactTag(
  event: RelayEvent,
  name: string,
  length = 2,
): string[] | null {
  const tags = event.tags.filter((tag) => tag[0] === name);
  return tags.length === 1 && tags[0].length === length ? tags[0] : null;
}

/** Copy canonical fields before verification so mutation caches cannot authenticate data. */
export function verifiedEvent(input: unknown): RelayEvent | null {
  if (!input || typeof input !== "object") return null;
  const raw = input as Record<string, unknown>;
  if (
    typeof raw.id !== "string" ||
    !HEX64.test(raw.id) ||
    typeof raw.pubkey !== "string" ||
    !HEX64.test(raw.pubkey) ||
    typeof raw.sig !== "string" ||
    !/^[0-9a-f]{128}$/.test(raw.sig) ||
    !integer(raw.created_at) ||
    raw.kind !== 30623 ||
    !safeText(raw.content) ||
    !Array.isArray(raw.tags) ||
    !raw.tags.every((tag) => Array.isArray(tag) && tag.every(safeText))
  )
    return null;
  const event: RelayEvent = {
    id: raw.id,
    pubkey: raw.pubkey,
    created_at: raw.created_at,
    kind: raw.kind,
    tags: (raw.tags as string[][]).map((tag) => [...tag]),
    content: raw.content,
    sig: raw.sig,
  };
  if (encoder.encode(JSON.stringify(event)).length > 192 * 1024) return null;
  return verifyEvent(event) ? event : null;
}

/** Byte-order comparison must not use JavaScript UTF-16 string ordering. */
function comparePath(left: string, right: string): number {
  const a = encoder.encode(left);
  const b = encoder.encode(right);
  for (let i = 0; i < Math.min(a.length, b.length); i += 1) {
    if (a[i] !== b[i]) return a[i] - b[i];
  }
  return a.length - b.length;
}

/** Source tuples are locators only after native signed-source authorization. */
export function validSourceRefs(value: unknown): boolean {
  if (!Array.isArray(value)) return false;
  let previous: [string, string, number, number, number] | null = null;
  for (const ref of value) {
    if (!Array.isArray(ref) || ref.length !== 5) return false;
    const [path, hash, bytes, start, end] = ref;
    if (
      !safeText(path) ||
      path.length === 0 ||
      path.includes("\\") ||
      path
        .split("/")
        .some((part) => part === "" || part === "." || part === "..") ||
      /^[A-Za-z]:/.test(path) ||
      typeof hash !== "string" ||
      !HEX64.test(hash) ||
      !integer(bytes) ||
      bytes === 0 ||
      bytes > 1024 * 1024 ||
      !integer(start) ||
      start < 1 ||
      !integer(end) ||
      end < start ||
      end > bytes
    )
      return false;
    if (previous) {
      const order = comparePath(previous[0], path);
      if (
        order > 0 ||
        (order === 0 &&
          (previous[1] !== hash ||
            previous[2] !== bytes ||
            previous[3] > start ||
            (previous[3] === start && previous[4] >= end)))
      )
        return false;
    }
    previous = ref as [string, string, number, number, number];
  }
  return true;
}

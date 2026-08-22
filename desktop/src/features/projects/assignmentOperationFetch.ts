import { relayClient } from "@/shared/api/relayClient";
import type { RelayEvent } from "@/shared/api/types";
import { KIND_TEXT_NOTE } from "@/shared/constants/kinds";
import {
  ISSUE_ASSIGNMENT_LABEL,
  ISSUE_UNASSIGNMENT_LABEL,
} from "./projectIssues.mjs";

type FetchEventsInput = Parameters<(typeof relayClient)["fetchEvents"]>[0];

const ASSIGNMENT_PAGE_LIMIT = 500;
const RELAY_MAX_PAGE_LIMIT = 1_000;
const ISSUE_ID_CHUNK_SIZE = 100;

function isAssignmentOperation(event: RelayEvent): boolean {
  return event.tags.some(
    (tag) =>
      tag[0] === "t" &&
      (tag[1] === ISSUE_ASSIGNMENT_LABEL ||
        tag[1] === ISSUE_UNASSIGNMENT_LABEL),
  );
}

export async function fetchAssignmentOperationEvents(
  issueIds: string[],
  fetchEvents: (
    filter: FetchEventsInput,
  ) => Promise<RelayEvent[]> = relayClient.fetchEvents.bind(relayClient),
): Promise<RelayEvent[]> {
  if (issueIds.length === 0) return [];
  const chunks: string[][] = [];
  for (let index = 0; index < issueIds.length; index += ISSUE_ID_CHUNK_SIZE) {
    chunks.push(issueIds.slice(index, index + ISSUE_ID_CHUNK_SIZE));
  }
  const pages = await Promise.all(
    chunks.map((chunk) => fetchIssueCommentsExhaustively(chunk, fetchEvents)),
  );
  const seen = new Map<string, RelayEvent>();
  for (const page of pages) {
    for (const event of page) {
      if (isAssignmentOperation(event) && !seen.has(event.id)) {
        seen.set(event.id, event);
      }
    }
  }
  return [...seen.values()];
}

async function fetchIssueCommentsExhaustively(
  issueIds: string[],
  fetchEvents: (filter: FetchEventsInput) => Promise<RelayEvent[]>,
): Promise<RelayEvent[]> {
  const seen = new Map<string, RelayEvent>();
  let limit = ASSIGNMENT_PAGE_LIMIT;
  let until: number | undefined;
  for (;;) {
    const page = await fetchEvents({
      kinds: [KIND_TEXT_NOTE],
      "#e": issueIds,
      limit,
      ...(until === undefined ? {} : { until }),
    });
    for (const event of page) {
      if (!seen.has(event.id)) seen.set(event.id, event);
    }
    if (page.length < limit) break;
    const oldest = Math.min(...page.map((event) => event.created_at));
    if (until === undefined || oldest < until) {
      until = oldest;
      continue;
    }
    if (limit < RELAY_MAX_PAGE_LIMIT) {
      limit = RELAY_MAX_PAGE_LIMIT;
      continue;
    }
    throw new Error(
      "Could not load assignment history: more than a full relay page of issue comments share one timestamp.",
    );
  }
  return [...seen.values()];
}

export function mergeEventsById(
  base: RelayEvent[],
  extra: RelayEvent[],
): RelayEvent[] {
  const ids = new Set(base.map((event) => event.id));
  return [...base, ...extra.filter((event) => !ids.has(event.id))];
}

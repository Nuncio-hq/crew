// Response-side budgets for the Protocol B live-relay acceptance harness.
//
// The live harness talks to an operator-supplied disposable relay, so every
// bound here is about what this process is willing to *hold* from a remote
// answer, not about what the request claimed. Serializing a filter only proves
// the request is small; a relay can answer any filter with an unbounded event
// stream. These helpers live in their own module so the same code that runs
// against the live relay is the code the unit test exercises.

// Largest signed Wiki event envelope the relay contract admits.
export const MAX_EVENT_BYTES = 192 * 1024;
// Largest response this process retains for a single subscription.
export const MAX_RESPONSE_BYTES = 1024 * 1024;
// Largest request filter this process is willing to send.
export const MAX_FILTER_BYTES = 1024 * 1024;
// Largest whole-graph read (head + manifest + every page batch).
export const MAX_GRAPH_BYTES = 64 * 1024 * 1024;
// Page-count bound of one Wiki publication.
export const MAX_PUBLICATION_PAGES = 256;
// Page IDs requested per batched read.
export const PAGE_QUERY_BATCH = 4;
// Ceiling on events retained from one subscription. The widest legitimate read
// is a page batch asking for `PAGE_QUERY_BATCH` IDs with one extra slot so a
// duplicate live row is observable rather than silently truncated.
export const MAX_RESPONSE_EVENTS = PAGE_QUERY_BATCH + 1;
// NIP-11 descriptors are small, trusted-shape documents. Anything larger is
// treated as a hostile or misconfigured origin rather than parsed.
export const MAX_DESCRIPTOR_BYTES = 64 * 1024;

// Raised when a remote answer would exceed a retention bound.
export class BudgetExceededError extends Error {
  constructor(message) {
    super(message);
    this.name = "BudgetExceededError";
  }
}

// Byte length of the canonical JSON envelope this process would retain.
export function envelopeBytes(value) {
  return Buffer.byteLength(JSON.stringify(value));
}

export function assertFilterBudget(filter) {
  const bytes = envelopeBytes(filter);
  if (bytes > MAX_FILTER_BYTES) {
    throw new BudgetExceededError(
      `Wiki graph request filter is ${bytes} bytes, above the ${MAX_FILTER_BYTES}-byte request budget`,
    );
  }
  return bytes;
}

// Retained-event ceiling for one subscription: never above the absolute cap,
// and never above what the filter itself asked for.
export function responseEventCap(filter) {
  const requested = Number.isInteger(filter?.limit)
    ? filter.limit
    : MAX_RESPONSE_EVENTS;
  return Math.max(1, Math.min(requested, MAX_RESPONSE_EVENTS));
}

// Accumulator shared by every read that composes one Wiki graph.
export function createGraphBudget({
  maxBytes = MAX_GRAPH_BYTES,
  maxPages = MAX_PUBLICATION_PAGES,
} = {}) {
  return { maxBytes, maxPages, bytes: 0, events: 0 };
}

// Bounded collector for one subscription.
//
// Every bound is checked *before* the event is retained, so exceeding a budget
// never costs the caller the memory the budget was meant to deny.
export function createResponseCollector({
  maxEvents = MAX_RESPONSE_EVENTS,
  maxEventBytes = MAX_EVENT_BYTES,
  maxResponseBytes = MAX_RESPONSE_BYTES,
  graph = null,
} = {}) {
  const events = [];
  let bytes = 0;
  return {
    events,
    get bytes() {
      return bytes;
    },
    admit(event) {
      const size = envelopeBytes(event);
      if (size > maxEventBytes) {
        throw new BudgetExceededError(
          `relay answered with a ${size}-byte event, above the ${maxEventBytes}-byte envelope bound`,
        );
      }
      if (events.length >= maxEvents) {
        throw new BudgetExceededError(
          `relay answered with more than ${maxEvents} events for one filter`,
        );
      }
      if (bytes + size > maxResponseBytes) {
        throw new BudgetExceededError(
          `relay response would reach ${bytes + size} bytes, above the ${maxResponseBytes}-byte response budget`,
        );
      }
      if (graph) {
        if (graph.bytes + size > graph.maxBytes) {
          throw new BudgetExceededError(
            `Wiki graph read would reach ${graph.bytes + size} bytes, above the ${graph.maxBytes}-byte graph budget`,
          );
        }
        graph.bytes += size;
        graph.events += 1;
      }
      bytes += size;
      events.push(event);
      return event;
    },
  };
}

// Read a small trusted-shape JSON document without buffering an unbounded
// body. `response.json()` would read whatever the origin sends; this cancels
// the stream as soon as the cap is crossed.
export async function readBoundedJson(
  response,
  { maxBytes = MAX_DESCRIPTOR_BYTES } = {},
) {
  const declared = Number(response.headers?.get?.("content-length"));
  if (Number.isFinite(declared) && declared > maxBytes) {
    throw new BudgetExceededError(
      `declared body of ${declared} bytes is above the ${maxBytes}-byte descriptor bound`,
    );
  }
  const body = response.body;
  if (!body) {
    throw new BudgetExceededError("response had no readable body");
  }
  const reader = body.getReader();
  const chunks = [];
  let bytes = 0;
  try {
    for (;;) {
      const { done, value } = await reader.read();
      if (done) break;
      const chunk = Buffer.from(value);
      bytes += chunk.byteLength;
      if (bytes > maxBytes) {
        await reader.cancel("descriptor exceeded its size bound");
        throw new BudgetExceededError(
          `body exceeded the ${maxBytes}-byte descriptor bound`,
        );
      }
      chunks.push(chunk);
    }
  } finally {
    try {
      reader.releaseLock?.();
    } catch {
      // Releasing an already cancelled reader must not mask the budget error.
    }
  }
  return JSON.parse(Buffer.concat(chunks).toString("utf8"));
}

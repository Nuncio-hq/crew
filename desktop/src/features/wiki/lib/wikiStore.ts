import type { WikiJobState } from "./wikiEvents";

let jobs = new Map<string, WikiJobState>();
const listeners = new Set<() => void>();

function notify(): void {
  for (const listener of listeners) listener();
}

export function getWikiJobs(): Map<string, WikiJobState> {
  return jobs;
}

export function setWikiJob(next: WikiJobState): void {
  jobs = new Map(jobs);
  jobs.set(next.repoKey, next);
  notify();
}

export function subscribeWikiJobs(listener: () => void): () => void {
  listeners.add(listener);
  return () => {
    listeners.delete(listener);
  };
}

export function resetWikiStore(): void {
  jobs = new Map();
  notify();
}

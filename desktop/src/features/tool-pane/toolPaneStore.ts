import * as React from "react";
import { normalizeRelayUrl } from "@/shared/lib/normalizeRelayUrl";
import { normalizePubkey } from "@/shared/lib/pubkey";

import type { ToolPaneTab } from "./types";
import {
  threadToolPaneScopeKey,
  type ThreadToolPaneScope,
} from "./threadToolPaneScope";

type Snapshot = {
  open: boolean;
  tab: ToolPaneTab;
  poppedOut: boolean;
};

let snapshot: Snapshot = { open: false, tab: "sim", poppedOut: false };
const listeners = new Set<() => void>();
const selections = new Map<string, ToolPaneTab>();
let viewScope: ThreadToolPaneScope | null = null;
let viewKey: string | null = null;
let channelTab: ToolPaneTab = "sim";
let bindingSequence = 0;
let communityEpoch = 0;
let paneOpener: HTMLElement | null = null;

/** Capture before React mounts a modal and moves focus into it. */
export function getToolPaneOpener() {
  return paneOpener;
}

/** Capture at the start of an async query; community resets retire its result. */
export function getThreadToolPaneEpoch() {
  return communityEpoch;
}

function removeSelections(
  relayUrl: string,
  viewerPubkey: string,
  matches: (channelId: string, rootEventId: string) => boolean,
) {
  const relay = normalizeRelayUrl(relayUrl);
  const viewer = normalizePubkey(viewerPubkey);
  for (const key of selections.keys()) {
    const [recordRelay, recordViewer, channel, root] = JSON.parse(
      key,
    ) as string[];
    if (
      recordRelay !== relay ||
      recordViewer !== viewer ||
      !matches(channel, root)
    )
      continue;
    selections.delete(key);
    if (key === viewKey) {
      viewKey = null;
      publish({ open: false, poppedOut: false, tab: "context" });
    }
  }
}

/** Only complete authoritative channel reconciliation may establish absence. */
export function reconcileThreadToolPaneChannels(
  relayUrl: string,
  viewerPubkey: string,
  channels: readonly { id: string }[],
  epoch: number,
) {
  if (epoch !== communityEpoch) return;
  const present = new Set(channels.map((channel) => channel.id.toLowerCase()));
  removeSelections(
    relayUrl,
    viewerPubkey,
    (channelId) => !present.has(channelId),
  );
}

/** Use only after a confirmed mutation; failed and previous-community work is inert. */
export function captureThreadToolPaneRemoval(
  relayUrl: string,
  viewerPubkey: string,
) {
  const epoch = communityEpoch;
  return (
    channelId: string | null,
    rootEventId?: string,
    removedMember?: string,
  ) => {
    if (
      !channelId ||
      epoch !== communityEpoch ||
      (removedMember &&
        normalizePubkey(removedMember) !== normalizePubkey(viewerPubkey))
    )
      return;
    removeSelections(
      relayUrl,
      viewerPubkey,
      (channel, root) =>
        channel === channelId.toLowerCase() &&
        (rootEventId === undefined || root === rootEventId),
    );
  };
}

/** Bind a mounted thread view; stale unmounts cannot close a newer view. */
export function bindThreadToolPaneView(scope: ThreadToolPaneScope) {
  setToolPaneView(scope);
  const binding = ++bindingSequence;
  return () => {
    if (binding === bindingSequence) setToolPaneView(null);
  };
}

/** Prevent a previous view's presentation from rendering during navigation. */
export function isCurrentThreadToolPaneView(scope: ThreadToolPaneScope) {
  return viewKey !== null && viewKey === threadToolPaneScopeKey(scope);
}

function remember(tab: ToolPaneTab) {
  if (!viewScope) {
    channelTab = tab;
    return;
  }
  if (!viewKey) return;
  selections.delete(viewKey);
  selections.set(viewKey, tab);
  if (selections.size > 128) {
    const oldest = selections.keys().next().value;
    if (oldest !== undefined) selections.delete(oldest);
  }
}

/** Navigation closes thread presentation; restoring selection never opens it. */
export function setToolPaneView(scope: ThreadToolPaneScope | null) {
  const key = scope ? threadToolPaneScopeKey(scope) : null;
  if (key === viewKey && Boolean(scope) === Boolean(viewScope)) return;
  bindingSequence += 1;
  paneOpener = null;
  viewScope = scope;
  viewKey = key;
  const tab = scope
    ? ((key ? selections.get(key) : null) ?? "context")
    : channelTab;
  publish({ open: false, poppedOut: false, tab });
}

/** Called only for confirmed removal/revocation, never loading or query errors. */
export function invalidateThreadToolPaneScope(scope: ThreadToolPaneScope) {
  const key = threadToolPaneScopeKey(scope);
  if (!key) return;
  selections.delete(key);
  if (key === viewKey) {
    viewKey = null;
    publish({ open: false, poppedOut: false, tab: "context" });
  }
}

function publish(next: Snapshot) {
  snapshot = next;
  for (const listener of [...listeners]) listener();
}

export function openToolPane(tab?: ToolPaneTab, scope?: ThreadToolPaneScope) {
  if (scope) setToolPaneView(scope);
  if (viewScope && !viewKey) return;
  if (
    !snapshot.open &&
    typeof document !== "undefined" &&
    typeof HTMLElement !== "undefined"
  ) {
    paneOpener =
      document.activeElement instanceof HTMLElement
        ? document.activeElement
        : null;
  }
  remember(tab ?? snapshot.tab);
  publish({
    ...snapshot,
    open: true,
    tab: tab ?? snapshot.tab,
  });
}

export function closeToolPane() {
  if (!snapshot.open && !snapshot.poppedOut) return;
  publish({ ...snapshot, open: false, poppedOut: false });
}

export function dismiss(onDismiss: () => void, keepToolPane: boolean) {
  if (!keepToolPane) closeToolPane();
  onDismiss();
}

export function toggleToolPane(tab?: ToolPaneTab) {
  if (snapshot.open && (!tab || tab === snapshot.tab)) {
    closeToolPane();
    return;
  }
  openToolPane(tab);
}

export function setToolPaneTab(tab: ToolPaneTab) {
  openToolPane(tab);
}

export function setToolPanePoppedOut(poppedOut: boolean) {
  if (viewScope) return;
  publish({ ...snapshot, poppedOut, open: poppedOut ? true : snapshot.open });
}

function subscribe(listener: () => void) {
  listeners.add(listener);
  return () => {
    listeners.delete(listener);
  };
}

export function useToolPane() {
  return React.useSyncExternalStore(subscribe, getToolPaneSnapshot);
}

export function resetToolPaneForTests() {
  selections.clear();
  viewScope = null;
  viewKey = null;
  channelTab = "sim";
  bindingSequence += 1;
  communityEpoch += 1;
  paneOpener = null;
  publish({ open: false, tab: "sim", poppedOut: false });
}

export function getToolPaneSnapshot() {
  return snapshot;
}

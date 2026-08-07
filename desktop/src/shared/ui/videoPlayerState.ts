// Inline/review playback state survives player remounts (for example route
// swaps, optimistic-to-acked row replacement, or markdown context refreshes)
// without leaking across community switches.
const inlinePlaybackPositions = new Map<string, number>();
const openReviewKeys = new Set<string>();
const reviewPlaybackPositions = new Map<string, number>();
const openSpeedMenuKeys = new Set<string>();

export function resetVideoPlayerState(): void {
  inlinePlaybackPositions.clear();
  openReviewKeys.clear();
  reviewPlaybackPositions.clear();
  openSpeedMenuKeys.clear();
}

export function isSpeedMenuOpen(key: string): boolean {
  return openSpeedMenuKeys.has(key);
}

export function setSpeedMenuOpen(key: string, open: boolean): void {
  if (open) {
    openSpeedMenuKeys.add(key);
  } else {
    openSpeedMenuKeys.delete(key);
  }
}

/**
 * True while any video overlay (review dialog or speed menu) is open. These
 * overlays render through portals owned by a timeline row's VideoPlayer, so
 * evicting that row unmounts the overlay out from under the user.
 */
export function hasOpenVideoOverlays(): boolean {
  return openReviewKeys.size > 0 || openSpeedMenuKeys.size > 0;
}

export function getInlinePlaybackPosition(key: string): number | undefined {
  return inlinePlaybackPositions.get(key);
}

export function saveInlinePlaybackPosition(
  key: string,
  seconds: number,
  options?: { ignoreResetToZero?: boolean },
): void {
  if (!Number.isFinite(seconds)) {
    return;
  }
  const nextSeconds = Math.max(0, seconds);
  const savedSeconds = inlinePlaybackPositions.get(key) ?? 0;
  if (options?.ignoreResetToZero && nextSeconds === 0 && savedSeconds > 0) {
    return;
  }
  inlinePlaybackPositions.set(key, nextSeconds);
}

export function isVideoReviewOpen(key: string): boolean {
  return openReviewKeys.has(key);
}

export function setVideoReviewOpen(key: string, open: boolean): void {
  if (open) {
    openReviewKeys.add(key);
  } else {
    openReviewKeys.delete(key);
  }
}

export function getReviewPlaybackPosition(key: string): number | undefined {
  return reviewPlaybackPositions.get(key);
}

export function saveReviewPlaybackPosition(
  key: string,
  seconds: number,
): number {
  const nextSeconds = Number.isFinite(seconds) ? Math.max(0, seconds) : 0;
  reviewPlaybackPositions.set(key, nextSeconds);
  return nextSeconds;
}

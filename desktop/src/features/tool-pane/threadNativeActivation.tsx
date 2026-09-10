import * as React from "react";
import { toast } from "sonner";
import {
  browserClose,
  browserOpen,
  setBrowserBounds,
  simEnsureDevice,
  simSetPaneVisible,
} from "./governorClient";
import type { CanvasTooling, SimLifecycle } from "./types";

/** Activation belongs to this mounted view; selecting or restoring it is inert. */
export function useThreadNativeActivation(
  channelId: string,
  root?: string | null,
) {
  const scope = JSON.stringify([channelId, root ?? null]);
  const [activatedScope, setActivatedScope] = React.useState<string | null>(
    null,
  );
  const [failure, setFailure] = React.useState<{
    scope: string;
    message: string;
  } | null>(null);
  React.useLayoutEffect(() => {
    void scope; // Returning to a scope must require a fresh explicit activation.
    setActivatedScope(null);
    setFailure(null);
  }, [scope]);
  const active = !root || activatedScope === scope;
  const reportError = React.useCallback(
    (error: unknown) => {
      setActivatedScope((current) => (current === scope ? null : current));
      setFailure({
        scope,
        message: error instanceof Error ? error.message : String(error),
      });
    },
    [scope],
  );
  return {
    active,
    error: failure?.scope === scope ? failure.message : null,
    reportError,
    activate: () => {
      setFailure(null);
      setActivatedScope(scope);
    },
    scope,
  };
}

/** Explicit instrument action; no resource work is performed by rendering. */
export function NativeToolActivation({
  name,
  onActivate,
}: {
  name: string;
  onActivate: () => void;
}) {
  return (
    <button
      className="rounded-md border border-border px-3 py-2 text-sm"
      onClick={onActivate}
      type="button"
    >
      Activate {name}
    </button>
  );
}

/** Preserve governor hide cleanup when the activated native browser unmounts. */
export function useBrowserNativePreview({
  channelId,
  url,
  active,
  frameRef,
  reportError,
}: {
  channelId: string;
  url: string | null;
  active: boolean;
  frameRef: React.RefObject<HTMLDivElement | null>;
  reportError: (error: unknown) => void;
}) {
  const generation = React.useRef(0);
  React.useEffect(() => {
    const attempt = ++generation.current;
    if (!active || !url) return;
    const failCurrent = (error: unknown) => {
      if (attempt === generation.current) reportError(error);
    };
    void browserOpen(channelId, url).catch((error) => {
      failCurrent(error);
    });
    const node = frameRef.current;
    const sync = () => {
      if (!node || attempt !== generation.current) return;
      const rect = node.getBoundingClientRect();
      void setBrowserBounds(
        channelId,
        rect.x,
        rect.y,
        rect.width,
        rect.height,
      ).catch(failCurrent);
    };
    const observer = new ResizeObserver(sync);
    if (node) observer.observe(node);
    sync();
    return () => {
      generation.current += 1;
      observer.disconnect();
      void browserClose(channelId).catch((error) =>
        reportNativeCleanupFailure("Browser", channelId, error),
      );
    };
  }, [active, channelId, frameRef, reportError, url]);
}

// Cleanup belongs to the channel governor, not the next view's activation.
function reportNativeCleanupFailure(
  instrument: string,
  channelId: string,
  error: unknown,
) {
  toast.error(
    `${instrument} hide failed for channel ${channelId}: ${error instanceof Error ? error.message : String(error)}`,
    { duration: Infinity, id: `native-hide:${instrument}:${channelId}` },
  );
}

/** Channel compatibility keeps find-or-create on mount; threads use the create card. */
export function useSimNativePresentation({
  channelId,
  channelName,
  threadRootId,
  tooling,
  face,
  active,
  reportError,
}: {
  channelId: string;
  channelName: string;
  threadRootId?: string | null;
  tooling: CanvasTooling | null;
  face: SimLifecycle | "bridge-missing";
  active: boolean;
  reportError: (error: unknown) => void;
}) {
  const presentationGeneration = React.useRef(0);
  React.useEffect(() => {
    if (threadRootId) return;
    let current = true;
    void simEnsureDevice({
      channelId,
      channelName,
      deviceType: tooling?.simulator?.deviceType,
      runtime: tooling?.simulator?.runtime,
    }).catch((error) => {
      if (current) reportError(error);
    });
    return () => {
      current = false;
    };
  }, [
    channelId,
    channelName,
    threadRootId,
    tooling?.simulator?.deviceType,
    tooling?.simulator?.runtime,
    reportError,
  ]);
  React.useEffect(() => {
    const attempt = ++presentationGeneration.current;
    if (!active || face === "bridge-missing" || face === "absent") return;
    void simSetPaneVisible(channelId, true).catch((error) => {
      if (attempt === presentationGeneration.current) reportError(error);
    });
    return () => {
      presentationGeneration.current += 1;
      void simSetPaneVisible(channelId, false).catch((error) =>
        reportNativeCleanupFailure("Simulator", channelId, error),
      );
    };
  }, [active, channelId, face, reportError]);
}

import * as PopoverPrimitive from "@radix-ui/react-popover";
import { Check } from "lucide-react";
import * as React from "react";

import { cn } from "@/shared/lib/cn";

import { isSpeedMenuOpen, setSpeedMenuOpen } from "./videoPlayerState";

export const PLAYBACK_SPEEDS = [2, 1.75, 1.5, 1.25, 1, 0.75, 0.5, 0.25];

export function formatPlaybackSpeed(speed: number): string {
  return `${speed}x`;
}

export function isSupportedPlaybackSpeed(speed: number): boolean {
  return PLAYBACK_SPEEDS.some((option) => option === speed);
}

const MENU_SURFACE_CLASS =
  "z-50 w-28 rounded-xl border border-white/10 bg-black/85 p-1 text-white outline-hidden backdrop-blur-xl";

function SpeedMenuItems({
  playbackSpeed,
  onSelect,
}: {
  playbackSpeed: number;
  onSelect: (speed: number) => void;
}) {
  return (
    <>
      <div className="px-2 pb-1 pt-1 text-2xs font-medium text-white/55">
        Speed
      </div>
      <div className="grid gap-0.5">
        {PLAYBACK_SPEEDS.map((speed) => {
          const speedLabel = formatPlaybackSpeed(speed);
          const selected = speed === playbackSpeed;
          return (
            <button
              aria-pressed={selected}
              className={cn(
                "flex h-8 w-full items-center justify-between rounded-lg px-2 text-left text-xs font-medium tabular-nums text-white transition-colors hover:bg-white/15 focus-visible:outline-hidden focus-visible:ring-2 focus-visible:ring-white/60",
                selected && "bg-white/15",
              )}
              key={speed}
              type="button"
              onClick={(event) => {
                event.stopPropagation();
                onSelect(speed);
              }}
            >
              <span>{speedLabel}</span>
              {selected ? <Check className="h-3.5 w-3.5" /> : null}
            </button>
          );
        })}
      </div>
    </>
  );
}

export const PlaybackSpeedControl = React.memo(function PlaybackSpeedControl({
  playbackSpeed,
  onPlaybackSpeedChange,
  size = "inline",
  testId,
}: {
  playbackSpeed: number;
  onPlaybackSpeedChange: (speed: number) => void;
  size?: "inline" | "review";
  testId: string;
}) {
  // Module-cache seeded: this control lives inside a virtualized timeline row
  // (even the review dialog's instance — the portal owner is the row's
  // VideoPlayer), so a row remount must not close a menu mid-interaction.
  const [open, setOpenState] = React.useState(() => isSpeedMenuOpen(testId));
  const setOpen = React.useCallback(
    (nextOpen: boolean) => {
      setSpeedMenuOpen(testId, nextOpen);
      setOpenState(nextOpen);
    },
    [testId],
  );
  const handleSelect = React.useCallback(
    (speed: number) => {
      onPlaybackSpeedChange(speed);
      setOpen(false);
    },
    [onPlaybackSpeedChange, setOpen],
  );
  const containerRef = React.useRef<HTMLDivElement | null>(null);
  const isReview = size === "review";

  // Review menus dismiss on outside pointerdown (the inline path lets Radix
  // handle this).
  React.useEffect(() => {
    if (!isReview || !open) return;
    const handlePointerDown = (event: PointerEvent) => {
      const container = containerRef.current;
      if (!container) return;
      if (event.target instanceof Node && container.contains(event.target)) {
        return;
      }
      setOpen(false);
    };
    document.addEventListener("pointerdown", handlePointerDown);
    return () => document.removeEventListener("pointerdown", handlePointerDown);
  }, [isReview, open, setOpen]);

  const label = formatPlaybackSpeed(playbackSpeed);
  const triggerSizeClass = isReview
    ? "h-8 min-w-11 rounded-lg px-2 text-xs"
    : "h-7 min-w-10 rounded-md px-1.5 text-2xs";
  const trigger = (
    <button
      aria-expanded={open}
      aria-haspopup="menu"
      aria-label={`Playback speed: ${label}`}
      className={cn(
        "flex shrink-0 items-center justify-center font-semibold tabular-nums text-white transition-colors hover:bg-white/15 focus-visible:outline-hidden focus-visible:ring-2 focus-visible:ring-white/60",
        triggerSizeClass,
        open && "bg-white/15",
      )}
      data-testid={testId}
      type="button"
      onClick={
        isReview
          ? (event) => {
              event.stopPropagation();
              setOpen(!open);
            }
          : (event) => event.stopPropagation()
      }
    >
      {label}
    </button>
  );

  if (isReview) {
    // The review dialog sits inside a live message timeline: the row that
    // portals this dialog keeps re-rendering as live events arrive. A Radix
    // popover repositions its portaled content through Popper on every such
    // pass, which visibly jitters the menu and detaches its items
    // mid-interaction. The dialog has ample room, so an in-flow absolutely
    // positioned dropdown (no portal, no repositioning engine) is stable by
    // construction.
    return (
      <div className="relative" ref={containerRef}>
        {trigger}
        {open ? (
          <div
            className={cn(
              MENU_SURFACE_CLASS,
              "absolute bottom-full right-0 mb-2",
            )}
            data-testid={`${testId}-menu`}
            role="menu"
            tabIndex={-1}
            onClick={(event) => event.stopPropagation()}
            onKeyDown={(event) => {
              event.stopPropagation();
              if (event.key === "Escape") setOpen(false);
            }}
          >
            <SpeedMenuItems
              playbackSpeed={playbackSpeed}
              onSelect={handleSelect}
            />
          </div>
        ) : null}
      </div>
    );
  }

  return (
    <PopoverPrimitive.Root modal={false} onOpenChange={setOpen} open={open}>
      <PopoverPrimitive.Trigger asChild>{trigger}</PopoverPrimitive.Trigger>
      <PopoverPrimitive.Portal>
        <PopoverPrimitive.Content
          forceMount
          align="end"
          className={MENU_SURFACE_CLASS}
          data-testid={`${testId}-menu`}
          side="top"
          sideOffset={8}
          onClick={(event) => event.stopPropagation()}
        >
          <SpeedMenuItems
            playbackSpeed={playbackSpeed}
            onSelect={handleSelect}
          />
        </PopoverPrimitive.Content>
      </PopoverPrimitive.Portal>
    </PopoverPrimitive.Root>
  );
});

import type { OverlayFrame } from "./agentControlStore";

export function GhostCursorOverlay({
  overlay,
  channelId,
  instrument,
}: {
  overlay: OverlayFrame | null;
  channelId: string;
  instrument: "browser" | "sim";
}) {
  if (
    !overlay ||
    overlay.channelId !== channelId ||
    overlay.instrument !== instrument ||
    !overlay.point
  ) {
    return null;
  }
  const { x, y } = overlay.point;
  return (
    <div
      className="pointer-events-none absolute inset-0 overflow-hidden"
      data-testid={`${instrument}-ghost-cursor`}
    >
      <span
        className="absolute h-2 w-2 rounded-full bg-primary"
        style={{ left: x, top: y, transform: "translate(-50%, -50%)" }}
      />
      <span
        className="absolute h-8 w-8 rounded-full border-2 border-primary"
        data-testid={`${instrument}-tap-ripple`}
        style={{
          left: x,
          top: y,
          transform: "translate(-50%, -50%)",
          animation: "crew-tap-ripple 800ms ease-out 1",
        }}
      />
      <style>
        {
          "@keyframes crew-tap-ripple { from { opacity: 0.7; transform: translate(-50%, -50%) scale(0.4); } to { opacity: 0; transform: translate(-50%, -50%) scale(1.6); } }"
        }
      </style>
    </div>
  );
}

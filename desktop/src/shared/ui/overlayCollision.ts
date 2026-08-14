/**
 * Overlay collision contract (#205 P7).
 *
 * Dialogs, popovers, menus, and toasts must stay inside the 800×500 window
 * floor and must not clip off a screen edge. Radix `avoidCollisions` is on by
 * default; we still pass an explicit padding and a viewport max-width so a
 * `w-72` popover cannot overflow the window.
 */

export const OVERLAY_COLLISION_PADDING_PX = 8;

/** Tailwind classes: never wider/taller than the window minus a 1rem gutter. */
export const OVERLAY_VIEWPORT_MAX_CLASS =
  "max-w-[calc(100vw-2rem)] max-h-[min(var(--radix-popper-available-height,100vh),calc(100vh-2rem))]";

export const DIALOG_VIEWPORT_MAX_CLASS =
  "max-h-[calc(100dvh-2rem)] overflow-y-auto w-[calc(100vw-2rem)]";

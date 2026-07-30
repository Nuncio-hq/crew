# Spike 0003 — Tauri Project folder picker

Status: PASS
Date: 2026-07-30

## Question

Can Crew select a real local directory from the Buzz Tauri shell without a
Rust or capability change?

## Setup

- Disposable detached worktree at the approved Buzz baseline.
- Existing native `tauri-plugin-dialog` version `2.7.1`.
- Existing plugin registration and `dialog:default` capability.
- Exact JavaScript binding `@tauri-apps/plugin-dialog@2.7.1`.
- Minimal additive screen calling:

```ts
open({
  directory: true,
  multiple: false,
  title: "Crew Project workspace spike",
});
```

## Evidence

- Desktop TypeScript check passed.
- Vite production build passed.
- Unsigned debug `Buzz.app` bundle built and ran as a real Tauri application.
- Cancel returned `null`; the spike displayed `cancelled`.
- A directory with spaces and Vietnamese characters returned
  `/private/tmp/crew-dialog-spike.xN7dW0/Nuncio Crew Đồ án`.
- Relinking to a directory with spaces and a CJK character returned
  `/private/tmp/crew-dialog-spike.xN7dW0/Nuncio Crew 二`.
- Both selections returned native absolute paths, not `file://` URLs or arrays.

The macOS picker resolved the `/tmp` symlink to `/private/tmp`. Crew must store
the path returned by the picker without another normalization step.

## Result

The picker boundary is feasible with one JavaScript dependency addition.
No Rust, capability, or `tauri.conf.json` edit is required.

## Limit

Tauri grants selected-path scope at runtime, but that scope is not persisted
across an app restart. This passes metadata capture. It does not prove
proactive restart-time filesystem access, which remains outside this no-Rust
slice.

## Cleanup

The unsigned spike app was closed. Its disposable worktree, selected-folder
fixtures, and generated build artifacts were removed.

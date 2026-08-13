export type SimLifecycle =
  | "absent"
  | "shutdown"
  | "booting"
  | "booted"
  | "mirroring"
  | "deleted";

export type DevServerFace =
  | "running"
  | "idleStop"
  | "crashed"
  | "portConflict"
  | "setup";

export type ToolPaneTab = "pr" | "browser" | "sim";

export type BridgeStatus = {
  availability: "available" | "missing" | "failed" | string;
  binary: string | null;
  path: string | null;
  installHint: string | null;
  message: string | null;
};

export type SimHolding = {
  channelId: string;
  channelName: string | null;
  deviceName: string;
  udid: string | null;
  lifecycle: SimLifecycle;
  deviceType: string;
  runtime: string;
  foreign: boolean;
  diskBytes: number;
  lastUsedMs: number;
  idleDeadlineMs: number | null;
  paneVisible: boolean;
  mirroring: boolean;
  lastScreenshotDataUrl: string | null;
  bootElapsedMs: number | null;
};

export type DevServerHolding = {
  id: string;
  channelId: string;
  subject: string;
  command: string;
  port: number;
  url: string | null;
  face: DevServerFace;
  uptimeMs: number;
  idleDeadlineMs: number | null;
  lastLog: string[];
  portNote: string | null;
  crashCount: number;
  cwd: string;
};

export type WebviewHolding = {
  id: string;
  channelId: string;
  url: string;
  hidden: boolean;
  hiddenSinceMs: number | null;
  backend: string;
};

export type CapConflict = {
  kind: string;
  victimChannelId: string;
  victimName: string;
  incomingChannelId: string;
  incomingName: string;
  idleMs: number;
  keepToken: string;
};

export type GovernorPolicy = {
  maxBootedSims: number;
  maxMirrorStreams: number;
  mirrorFps: number;
  mirrorQuietFps: number;
  simIdleShutdownMs: number;
  streamPauseHiddenMs: number;
  hiddenWebviewCap: number;
  hiddenWebviewTtlMs: number;
  maxDevServers: number;
  devServerIdleMs: number;
  pruneUnusedMs: number;
};

export type GovernorStatus = {
  policy: GovernorPolicy;
  nowMs: number;
  sims: SimHolding[];
  servers: DevServerHolding[];
  webviews: WebviewHolding[];
  bootedCount: number;
  streamCount: number;
  serverCount: number;
  diskBytes: number;
  capConflict: CapConflict | null;
  pruneCandidates: SimHolding[];
  bridge: BridgeStatus;
  childWebviewAvailable: boolean;
};

export type CanvasTooling = {
  simulator?: { deviceType: string; runtime: string };
  devServer?: { command: string; readyPattern?: string };
  browserAllowlist?: string[];
};

export const DEFAULT_GOVERNOR_POLICY: GovernorPolicy = {
  maxBootedSims: 2,
  maxMirrorStreams: 1,
  mirrorFps: 20,
  mirrorQuietFps: 5,
  simIdleShutdownMs: 15 * 60_000,
  streamPauseHiddenMs: 2_000,
  hiddenWebviewCap: 2,
  hiddenWebviewTtlMs: 10 * 60_000,
  maxDevServers: 3,
  devServerIdleMs: 25 * 60_000,
  pruneUnusedMs: 30 * 24 * 60 * 60_000,
};

export const EMPTY_GOVERNOR_STATUS: GovernorStatus = {
  policy: DEFAULT_GOVERNOR_POLICY,
  nowMs: 0,
  sims: [],
  servers: [],
  webviews: [],
  bootedCount: 0,
  streamCount: 0,
  serverCount: 0,
  diskBytes: 0,
  capConflict: null,
  pruneCandidates: [],
  bridge: {
    availability: "missing",
    binary: null,
    path: null,
    installHint:
      "brew install baguette\nbrew tap facebook/fb && brew install idb-companion",
    message: null,
  },
  childWebviewAvailable: false,
};

export const VIEWPORT_PRESETS = [
  { id: "desktop", label: "Desktop", width: 1280 },
  { id: "iphone", label: "iPhone", width: 393 },
  { id: "ipad", label: "iPad", width: 820 },
] as const;

import { useAgentDirectory } from "./AgentDirectory";
import {
  GitBranch,
  GitMerge,
  GitPullRequestClosed,
  GitPullRequestDraft,
  MousePointer2,
  Pause,
  LoaderCircle,
  TriangleAlert,
  Inbox,
  BookOpen,
  Users,
  Workflow,
  Folder,
  Hash,
  ChevronDown,
  ChevronRight,
  Plus,
  Search,
  PanelLeft,
  PanelRight,
  ArrowLeft,
  ArrowUp,
  X,
  FileText,
  Globe,
  GitPullRequest,
  Terminal,
  Check,
  Circle,
  Sparkles,
  Code2,
  Asterisk,
  MessageSquare,
  ArrowUpRight,
  MoreHorizontal,
  Paperclip,
  Square,
  ChevronsUpDown,
  WifiOff,
  RefreshCw,
  ListChecks,
  Settings2,
  CheckCircle2,
  Clock,
  Play,
  RotateCcw,
  BookMarked,
  ExternalLink,
  Info,
} from "lucide-react";
const icons = {
  cursor: MousePointer2,
  pause: Pause,
  spinner: LoaderCircle,
  alert: TriangleAlert,
  inbox: Inbox,
  wiki: BookOpen,
  agents: Users,
  workflows: Workflow,
  folder: Folder,
  hash: Hash,
  down: ChevronDown,
  right: ChevronRight,
  plus: Plus,
  search: Search,
  sidebar: PanelLeft,
  panel: PanelRight,
  back: ArrowLeft,
  send: ArrowUp,
  close: X,
  file: FileText,
  browser: Globe,
  pr: GitPullRequest,
  branch: GitBranch,
  merge: GitMerge,
  "pr-closed": GitPullRequestClosed,
  "pr-draft": GitPullRequestDraft,
  terminal: Terminal,
  check: Check,
  circle: Circle,
  sparkles: Sparkles,
  code: Code2,
  asterisk: Asterisk,
  message: MessageSquare,
  out: ArrowUpRight,
  more: MoreHorizontal,
  attach: Paperclip,
  stop: Square,
  switch: ChevronsUpDown,
  offline: WifiOff,
  retry: RefreshCw,
  plan: ListChecks,
  settings: Settings2,
  done: CheckCircle2,
  clock: Clock,
  play: Play,
  reset: RotateCcw,
  guide: BookMarked,
  external: ExternalLink,
  info: Info,
};
export function Icon({ name, size = 16, ...props }) {
  const Component = icons[name] || Circle;
  return (
    <Component size={size} strokeWidth={1.65} aria-hidden="true" {...props} />
  );
}
export function IconButton({ icon, label, onClick, ...props }) {
  return (
    <button
      type="button"
      className="icon-button"
      title={label}
      aria-label={label}
      onClick={onClick}
      {...props}
    >
      <Icon name={icon} />
    </button>
  );
}
export function Avatar({ name = "Hermes", small = false }) {
  const { find } = useAgentDirectory();
  const runtime = find(name)?.runtime || name;
  const brands = { Hermes: "hermes", Claude: "claude", Codex: "codex" };
  return (
    <span className={`avatar ${small ? "small" : ""} ${runtime.toLowerCase()}`}>
      {brands[runtime] ? (
        <img src={`/brands/${brands[runtime]}.svg`} alt="" aria-hidden="true" />
      ) : (
        <Icon name="agents" size={small ? 15 : 19} />
      )}
    </span>
  );
}
export function Status({ state }) {
  const labels = {
    working: "Working",
    "needs-you": "Needs you",
    done: "Ready for review",
    completed: "Done",
    blocked: "Blocked",
    discussion: "Discussion",
    offline: "Disconnected",
    idle: "Available",
    stopped: "Stopped",
  };
  return (
    <span className={`status ${state}`}>
      <span className="status-dot" />
      {labels[state] || state}
    </span>
  );
}

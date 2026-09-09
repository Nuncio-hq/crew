// Review fixtures only. Production must project existing relay/task evidence.
export const sampleThreads = [
  {
    id: "workspace",
    title: "Rebuild the workspace",
    owner: "Hermes",
    status: "working",
    summary: "Build the sidebar and focused conversation layout.",
    next: "Codex is implementing navigation. Nothing needed from you.",
    replies: 8,
    prs: ["layout", "navigation"],
    criteria:
      "Both PRs reviewed and merged; navigation verified; Oscar accepts the result. Deployment is out of scope.",
  },
  {
    id: "release",
    title: "Choose the release scope",
    owner: "Hermes",
    status: "needs-you",
    summary: "The preview is ready; production rollout needs a decision.",
    next: "Oscar: choose staging only or include production.",
    replies: 6,
    prs: ["release"],
    criteria:
      "Agree release scope, verify the chosen environment, then accept the handoff.",
  },
  {
    id: "handoff",
    title: "Review the delivery checklist",
    owner: "Claude",
    status: "done",
    summary: "A short handoff guide with a real-world walkthrough.",
    next: "Oscar: review the result and accept, or request changes.",
    replies: 12,
    prs: [],
    criteria:
      "Guide delivered, walkthrough verified, founder accepts. No PR or deployment required.",
  },
  {
    id: "ci",
    title: "Repair the reconnect regression",
    owner: "Codex",
    status: "blocked",
    summary: "Reconnect test failed on the current PR revision.",
    next: "Codex owns the fix. No decision needed from you.",
    replies: 14,
    prs: ["reconnect"],
    criteria:
      "Reconnect test and required checks pass on current revision; PR merged and founder accepts.",
  },
  {
    id: "archive",
    title: "Fix unread thread navigation",
    owner: "Codex",
    status: "completed",
    summary: "Unread navigation verified and accepted.",
    next: "Accepted by Oscar · today 09:10. No further action.",
    replies: 9,
    prs: ["unread"],
    criteria: "Merged PR, verified navigation and explicit acceptance.",
    receipt:
      "Oscar accepted revision unread-c8 after keyboard and pointer walkthrough. Deployment was not required.",
  },
  {
    id: "discussion",
    title: "Ideas for the team wiki",
    owner: "Claude",
    status: "discussion",
    summary: "A conversation about how we organize knowledge.",
    next: "Discussion only. No work has been assigned.",
    replies: 3,
    prs: [],
    criteria:
      "No task or completion requirement is attached to this discussion.",
  },
  {
    id: "remote",
    title: "Check the remote preview",
    owner: "Hermes",
    status: "offline",
    summary: "Last update: preview verification was running.",
    next: "Updates disconnected · last seen 12 minutes ago. Current progress unknown.",
    replies: 4,
    prs: [],
    criteria:
      "Reconnect and collect current verification evidence before review.",
  },
];
export const pullRequests = {
  layout: {
    title: "Workspace layout",
    repo: "NuncioCrew / desktop",
    number: "401",
    state: "Open",
    head: "layout-a3",
    checks: "Passed",
    review: "Approved",
    deploy: "Not required",
  },
  navigation: {
    title: "Project navigation",
    repo: "NuncioCrew / desktop",
    number: "402",
    state: "Draft",
    head: "nav-b7",
    checks: "Running",
    review: "Pending",
    deploy: "Not required",
  },
  release: {
    title: "Preview release",
    repo: "NuncioCrew / web",
    number: "403",
    state: "Open",
    head: "release-d2",
    checks: "Passed",
    review: "Approved",
    deploy: "Staging passed · production not started",
  },
  reconnect: {
    title: "Reconnect recovery",
    repo: "NuncioCrew / desktop",
    number: "404",
    state: "Open",
    head: "retry-f4",
    checks: "Failed",
    review: "Changes requested",
    deploy: "Not required",
  },
  unread: {
    title: "Unread navigation",
    repo: "NuncioCrew / desktop",
    number: "405",
    state: "Merged",
    head: "unread-c8",
    checks: "Passed",
    review: "Approved",
    deploy: "Not required",
  },
};
export const needsAttention = (t) => ["needs-you", "done"].includes(t.status);
export function nextStep(t) {
  if (t.status === "completed")
    return (
      t.receipt || "Accepted by Oscar in this prototype. No further action."
    );
  if (t.status === "stopped") return "Run stopped; work is not complete.";
  return t.next;
}

const participantsByThread = {
  workspace: ["Hermes", "Codex", "Claude"],
  release: ["Hermes", "Codex", "Claude"],
  handoff: ["Claude", "Hermes"],
  ci: ["Codex", "Claude"],
  archive: ["Codex", "Claude"],
  discussion: ["Claude"],
  remote: ["Hermes"],
  launch: ["Hermes", "Claude", "Codex"],
  audience: ["Claude"],
  "customer-followup": ["Hermes", "Claude"],
  tracking: ["Codex", "Claude"],
};
// Sample channel canvas assignments (D-043), never inferred from thread ownership.
const productRoles = [
  {
    name: "Hermes",
    role: "Product lead",
    scope: "Coordinate product scope and handoffs in this channel.",
  },
  {
    name: "Codex",
    role: "Implementation",
    scope: "Implement and verify approved product changes.",
  },
  {
    name: "Claude",
    role: "Review",
    scope: "Review work and evidence in this channel.",
  },
];
export function channelTeam(project = "NuncioCrew", channel = "product") {
  if (project === "NuncioCrew" && ["product", "engineering"].includes(channel))
    return productRoles;
  if (project === "HeardBack" && channel === "marketing")
    return [
      {
        name: "Hermes",
        role: "Campaign lead",
        scope: "Coordinate campaign scope and handoffs.",
      },
      {
        name: "Claude",
        role: "Copy & positioning",
        scope: "Prepare and review campaign messaging.",
      },
      {
        name: "Codex",
        role: "Research",
        scope: "Check audience and measurement dependencies.",
      },
    ];
  if (project === "HeardBack" && channel === "customers")
    return [
      {
        name: "Hermes",
        role: "Customer lead",
        scope: "Own the customer response plan.",
      },
      {
        name: "Claude",
        role: "Customer research",
        scope: "Organize feedback and draft follow-ups.",
      },
    ];
  return [];
}
export function threadTeam(t, roles = channelTeam(t.project, t.channel)) {
  const names = t.participants || participantsByThread[t.id] || [];
  return names.map(
    (name) =>
      roles.find((m) => m.name === name) || {
        name,
        role: "No channel role",
        scope: "No assignment available in this channel.",
      },
  );
}

sampleThreads.push(
  {
    id: "launch",
    project: "HeardBack",
    channel: "marketing",
    title: "Prepare the HeardBack launch",
    owner: "Hermes",
    status: "working",
    summary: "Email, LinkedIn copy and audience for next Tuesday’s launch.",
    next: "Agree positioning, create the launch pack and verify dependencies.",
    replies: 0,
    prs: [],
    criteria:
      "Approved copy and audience; Tuesday schedule prepared; signup tracking verified; Oscar accepts the launch pack.",
  },
  {
    id: "audience",
    project: "HeardBack",
    channel: "marketing",
    title: "What are applicants struggling with?",
    owner: "Claude",
    status: "discussion",
    summary: "Research notes on tailored applications and the follow-up gap.",
    next: "Discuss the positioning before turning this into a campaign.",
    replies: 4,
    prs: [],
    criteria: "Research discussion; no delivery task assigned.",
  },
  {
    id: "customer-followup",
    project: "HeardBack",
    channel: "customers",
    title: "Follow up with the pilot group",
    owner: "Hermes",
    status: "working",
    summary: "Collect feedback from the first five pilot participants.",
    next: "Claude is grouping feedback; Hermes owns the response plan.",
    replies: 7,
    prs: [],
    criteria:
      "Feedback summarized, response drafted and founder reviews the follow-up.",
  },
  {
    id: "tracking",
    project: "NuncioCrew",
    channel: "engineering",
    title: "Add signup tracking for the launch",
    owner: "Codex",
    status: "working",
    summary:
      "A small engineering dependency for HeardBack’s marketing campaign.",
    next: "Implement signup tracking and verify staging.",
    replies: 0,
    prs: ["tracking"],
    criteria:
      "No duplicate signup events or personal data; checks pass; staging verified; Oscar accepts the revision.",
  },
);
pullRequests.tracking = {
  title: "Signup conversion tracking",
  repo: "HeardBack / web",
  number: "406",
  state: "Open",
  head: "tracking-a1",
  checks: "Running",
  review: "Pending",
  deploy: "Staging pending",
};

// Sample checkout evidence; non-code threads deliberately have no checkout.
const workspaceExamples = {
  workspace: {
    repo: "crew",
    branch: "crew/workspace-layout",
    name: "workspace-layout",
    path: "/sample/worktrees/crew/workspace-layout",
    additions: 128,
    deletions: 42,
  },
  ci: {
    repo: "crew",
    branch: "fix/reconnect-recovery",
    name: "reconnect-recovery",
    path: "/sample/worktrees/crew/reconnect-recovery",
    additions: 34,
    deletions: 9,
  },
  archive: {
    repo: "crew",
    branch: "fix/unread-navigation",
    name: "unread-navigation",
    path: "/sample/worktrees/crew/unread-navigation",
    state: "removed",
  },
  tracking: {
    repo: "heardback",
    branch: "feat/signup-tracking",
    name: "signup-tracking",
    path: "/sample/worktrees/heardback/signup-tracking",
    additions: 46,
    deletions: 8,
  },
};
for (const thread of sampleThreads)
  if (workspaceExamples[thread.id])
    thread.workspace = workspaceExamples[thread.id];
const prDiffs = {
  layout: ["crew/workspace-layout", 284, 91],
  navigation: ["crew/project-navigation", 156, 38],
  release: ["release/preview", 18, 4],
  reconnect: ["fix/reconnect-recovery", 64, 12],
  unread: ["fix/unread-navigation", 71, 23],
  tracking: ["feat/signup-tracking", 46, 8],
};
for (const [id, [branch, additions, deletions]] of Object.entries(prDiffs))
  Object.assign(pullRequests[id], {
    branch,
    additions,
    deletions,
    base: "main",
  });
pullRequests.abandoned = {
  title: "Unread navigation — superseded approach",
  repo: "NuncioCrew / desktop",
  number: "400",
  state: "Closed",
  head: "old-d1",
  branch: "spike/unread-navigation",
  base: "main",
  checks: "Passed",
  review: "Not merged",
  deploy: "Not required",
  additions: 39,
  deletions: 17,
};
sampleThreads.find((t) => t.id === "archive").prs.push("abandoned");

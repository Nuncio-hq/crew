import { activityAt } from "./activity-model.js";
import { channelTeam, sampleThreads } from "./thread-model.js";
export const FILM_DURATION = 132;
export const filmChapters = [
  { at: 0, title: "The brief" },
  { at: 18, title: "Copy & research" },
  { at: 34, title: "Campaign copy" },
  { at: 51, title: "Engineering handoff" },
  { at: 85, title: "Marketing review" },
  { at: 118, title: "Delivery" },
];
const view = [
  {
    at: 0,
    screen: "channel",
    project: "HeardBack",
    channel: "marketing",
    title: "A new campaign starts in #marketing",
    target: "composer",
  },
  {
    at: 9,
    screen: "channel",
    project: "HeardBack",
    channel: "marketing",
    title: "Oscar sends the brief. A new thread appears.",
    target: "send",
  },
  {
    at: 11,
    screen: "thread",
    project: "HeardBack",
    channel: "marketing",
    thread: "launch",
    title: "Hermes takes ownership and brings in the team",
    target: "thread-launch",
  },
  {
    at: 25,
    screen: "thread",
    project: "HeardBack",
    channel: "marketing",
    thread: "launch",
    title: "Oscar chooses a direction while the team works",
    target: "composer",
  },
  {
    at: 34,
    screen: "thread",
    project: "HeardBack",
    channel: "marketing",
    thread: "launch",
    tool: "file",
    title: "Claude turns the chosen direction into launch copy",
    target: "tool-file",
  },
  {
    at: 43,
    screen: "thread",
    project: "HeardBack",
    channel: "marketing",
    thread: "launch",
    title: "Research uncovers a small engineering dependency",
    target: "channel-engineering",
  },
  {
    at: 51,
    screen: "channel",
    project: "NuncioCrew",
    channel: "engineering",
    title: "The team hands tracking work to #engineering",
    target: "channel-engineering",
  },
  {
    at: 54,
    screen: "thread",
    project: "NuncioCrew",
    channel: "engineering",
    thread: "tracking",
    title: "Codex works inside the linked engineering thread",
    target: "thread-tracking",
  },
  {
    at: 62,
    screen: "thread",
    project: "NuncioCrew",
    channel: "engineering",
    thread: "tracking",
    tool: "pr",
    title: "A pull request appears with checks still running",
    target: "tool-pr",
  },
  {
    at: 68,
    screen: "thread",
    project: "NuncioCrew",
    channel: "engineering",
    thread: "tracking",
    tool: "workflows",
    title: "A failing check stays visible while Codex repairs it",
    target: "tool-workflows",
  },
  {
    at: 76,
    screen: "thread",
    project: "NuncioCrew",
    channel: "engineering",
    thread: "tracking",
    title: "Oscar reviews the evidence before accepting the fix",
    target: "composer",
  },
  {
    at: 85,
    screen: "channel",
    project: "HeardBack",
    channel: "marketing",
    title: "Back in marketing, the campaign is ready for review",
    target: "channel-marketing",
  },
  {
    at: 88,
    screen: "thread",
    project: "HeardBack",
    channel: "marketing",
    thread: "launch",
    tool: "file",
    title: "The launch pack brings copy, audience and timing together",
    target: "thread-launch",
  },
  {
    at: 94,
    screen: "thread",
    project: "HeardBack",
    channel: "marketing",
    thread: "launch",
    title: "Oscar approves the plan in the conversation",
    target: "composer",
  },
  {
    at: 104,
    screen: "thread",
    project: "HeardBack",
    channel: "marketing",
    thread: "launch",
    tool: "file",
    title: "The team prepares the agreed campaign schedule",
    target: "tool-file",
  },
  {
    at: 116,
    screen: "thread",
    project: "HeardBack",
    channel: "marketing",
    thread: "launch",
    title: "A final handoff, with ownership and evidence",
    target: "composer",
  },
  {
    at: 126,
    screen: "channel",
    project: "HeardBack",
    channel: "marketing",
    title: "Campaign prep is done. The conversations stay available.",
    target: "channel-marketing",
  },
];
const messages = [
  {
    at: 2,
    end: 8,
    author: "Oscar",
    scope: "thread:launch",
    composeScope: "channel:HeardBack:marketing",
    text: "Let’s launch HeardBack next Tuesday. I need email, LinkedIn copy and a clear audience. Hermes, own the plan.",
  },
  {
    at: 12,
    end: 17,
    author: "Hermes",
    role: "Campaign lead",
    scope: "thread:launch",
    text: "I’ll own the launch pack. Claude: copy and positioning. Codex: audience research and tracking.",
  },
  {
    at: 19,
    end: 24,
    author: "Claude",
    role: "Copy & positioning",
    scope: "thread:launch",
    text: "Two angles: A — “Turn applications into conversations.” B — “Give your next application a better first impression.” I recommend A.",
  },
  {
    at: 26,
    end: 30,
    author: "Oscar",
    scope: "thread:launch",
    text: "Go with A. Keep it practical—no promises that everyone gets a reply.",
  },
  {
    at: 31,
    end: 36,
    author: "Claude",
    role: "Copy & positioning",
    scope: "thread:launch",
    text: "Agreed. I’ve drafted the email and LinkedIn post around that angle. Opening the copy beside this thread.",
  },
  {
    at: 39,
    end: 44,
    author: "Codex",
    role: "Audience research",
    scope: "thread:launch",
    text: "Audience: active job seekers with tailored applications. One dependency: the landing page needs a signup conversion event.",
  },
  {
    at: 46,
    end: 50,
    author: "Hermes",
    role: "Campaign lead",
    scope: "thread:launch",
    text: "I’ve linked an engineering thread for tracking. Claude can finish the launch pack while Codex handles that.",
  },
  {
    at: 50,
    end: 50,
    author: "Hermes",
    scope: "thread:tracking",
    text: "Add signup tracking for the HeardBack launch. Verify staging; keep personal data out of the event.",
  },
  {
    at: 55,
    end: 60,
    author: "Codex",
    role: "Implementation",
    scope: "thread:tracking",
    text: "The event now fires after a successful signup. PR #406 is ready; checks are running.",
  },
  {
    at: 68,
    end: 71,
    author: "Codex",
    role: "Implementation",
    scope: "thread:tracking",
    text: "One test caught a duplicate event on retry. I’m fixing the guard before handoff.",
  },
  {
    at: 73,
    end: 76,
    author: "Codex",
    role: "Implementation",
    scope: "thread:tracking",
    text: "Fixed. Checks pass on tracking-b2; staging records exactly one event. No email address is included.",
  },
  {
    at: 77,
    end: 82,
    author: "Oscar",
    scope: "thread:tracking",
    text: "The staging evidence looks right. Accept the fix and merge this revision.",
  },
  {
    at: 82,
    end: 84,
    author: "Codex",
    role: "Implementation",
    scope: "thread:tracking",
    text: "Merged tracking-b2. Verification is linked back to the campaign.",
  },
  {
    at: 89,
    end: 93,
    author: "Hermes",
    role: "Campaign lead",
    scope: "thread:launch",
    text: "Launch pack is ready: two drafts, audience notes and Tuesday’s schedule. Tracking is verified. Your review is the remaining step.",
  },
  {
    at: 95,
    end: 101,
    author: "Oscar",
    scope: "thread:launch",
    text: "Approved. Schedule the LinkedIn post for 9am Tuesday and email for 10am. Keep the first audience small.",
  },
  {
    at: 102,
    end: 107,
    author: "Claude",
    role: "Copy & positioning",
    scope: "thread:launch",
    text: "Copy is locked to version 3. Both drafts use the approved wording and link.",
  },
  {
    at: 109,
    end: 115,
    author: "Hermes",
    role: "Campaign lead",
    scope: "thread:launch",
    text: "The launch schedule is prepared. Owners, copy and tracking evidence are in the pack. No campaign results yet; reporting begins after launch.",
  },
  {
    at: 117,
    end: 122,
    author: "Oscar",
    scope: "thread:launch",
    text: "Looks good. Campaign prep is done. Bring the first results back to this thread after launch.",
  },
  {
    at: 123,
    end: 125,
    author: "Hermes",
    role: "Campaign lead",
    scope: "thread:launch",
    text: "Accepted. I’ll own the follow-up report; the team’s work stays linked here.",
  },
];
const changes = [
  {
    at: 8,
    id: "launch",
    status: "working",
    next: "Hermes is assembling the campaign team.",
  },
  {
    at: 18,
    id: "launch",
    status: "working",
    next: "Claude owns copy; Codex is checking audience and tracking.",
  },
  {
    at: 44,
    id: "launch",
    status: "working",
    next: "Copy is progressing. Signup tracking has a linked engineering dependency.",
  },
  {
    at: 50,
    id: "tracking",
    status: "working",
    next: "Codex owns signup tracking for the marketing launch.",
  },
  {
    at: 68,
    id: "tracking",
    status: "blocked",
    next: "A duplicate event test failed. Codex is repairing it.",
  },
  {
    at: 73,
    id: "tracking",
    status: "working",
    next: "Fix pushed; verifying the new revision.",
  },
  {
    at: 76,
    id: "tracking",
    status: "done",
    next: "Staging verified. Oscar: review the fix.",
  },
  {
    at: 83,
    id: "tracking",
    status: "completed",
    next: "Oscar accepted tracking-b2; merged with staging evidence.",
    receipt:
      "Oscar accepted tracking-b2 at 09:12 in this simulated story. Staging verified; PR #406 merged.",
  },
  {
    at: 85,
    id: "launch",
    status: "done",
    next: "Launch pack ready. Oscar: review copy, audience and schedule.",
  },
  {
    at: 101,
    id: "launch",
    status: "working",
    next: "Plan approved. Claude and Hermes are preparing the schedule.",
  },
  {
    at: 115,
    id: "launch",
    status: "done",
    next: "Schedule and evidence ready for final acceptance.",
  },
  {
    at: 122,
    id: "launch",
    status: "completed",
    next: "Campaign prep accepted. Reporting follows after launch.",
    receipt:
      "Oscar accepted launch pack v3 at 09:18 in this simulated story. Copy, schedule and tracking verified; delivery metrics are not yet available.",
  },
];
const pointerCues = [
  { at: 0, target: "composer" },
  { at: 8, target: "send" },
  { at: 10, target: "thread-launch" },
  { at: 25, target: "composer" },
  { at: 30, target: "send" },
  { at: 33, target: "open-tools" },
  { at: 34, target: "tool-file" },
  { at: 50, target: "channel-engineering" },
  { at: 53, target: "thread-tracking" },
  { at: 61, target: "open-tools" },
  { at: 62, target: "tool-pr" },
  { at: 67, target: "tool-workflows" },
  { at: 76, target: "composer" },
  { at: 82, target: "send" },
  { at: 84, target: "channel-marketing" },
  { at: 87, target: "thread-launch" },
  { at: 88, target: "tool-file" },
  { at: 94, target: "composer" },
  { at: 101, target: "send" },
  { at: 103, target: "open-tools" },
  { at: 104, target: "tool-file" },
  { at: 116, target: "composer" },
  { at: 122, target: "send" },
  { at: 125, target: "channel-marketing" },
];
export function filmFrameAt(time) {
  const timeNow = Math.max(0, Math.min(FILM_DURATION, time));
  const scene = [...view].reverse().find((v) => v.at <= timeNow);
  const sceneIndex = view.indexOf(scene);
  const pointer = [...pointerCues].reverse().find((c) => c.at <= timeNow);
  const resultMessages = {};
  let draft = "",
    typing = null;
  for (const m of messages) {
    if (timeNow < m.at) continue;
    const duration = m.end - m.at;
    const progress =
      duration === 0 ? 1 : Math.min(1, (timeNow - m.at) / duration);
    if (m.author === "Oscar" && progress < 1) {
      const currentScope =
        scene.screen === "thread"
          ? `thread:${scene.thread}`
          : `channel:${scene.project}:${scene.channel}`;
      if ((m.composeScope || m.scope) === currentScope)
        draft = m.text.slice(0, Math.floor(m.text.length * progress));
      continue;
    }
    if (m.author !== "Oscar" && progress < 0.16) {
      if (m.scope === `thread:${scene.thread}`) typing = m.author;
      continue;
    }
    const shown =
      m.author === "Oscar" || progress >= 1
        ? m.text
        : m.text.slice(
            0,
            Math.max(1, Math.floor((m.text.length * (progress - 0.16)) / 0.84)),
          );
    (resultMessages[m.scope] ||= []).push({
      author: m.author,
      role: channelTeam(
        m.scope === "thread:launch" ? "HeardBack" : "NuncioCrew",
        m.scope === "thread:launch" ? "marketing" : "engineering",
      ).find((r) => r.name === m.author)?.role,
      text: shown,
      streaming: progress < 1,
      time: `09:${String(3 + Math.floor(m.at / 8)).padStart(2, "0")}`,
    });
  }
  const threads = sampleThreads
    .filter((t) => t.id !== "launch" || timeNow >= 8)
    .filter((t) => t.id !== "tracking" || timeNow >= 50)
    .map((t) => {
      const patch = changes
        .filter((c) => c.id === t.id && c.at <= timeNow)
        .reduce((a, c) => ({ ...a, ...c }), {});
      const history = resultMessages[`thread:${t.id}`];
      return {
        ...t,
        ...patch,
        activity: activityAt(t, timeNow, true),
        ...(t.id === "tracking"
          ? {
              workspace: {
                ...t.workspace,
                additions: timeNow >= 64 && timeNow < 73 ? 46 : 0,
                deletions: timeNow >= 64 && timeNow < 73 ? 8 : 0,
              },
            }
          : {}),
        ...(history ? { replies: Math.max(0, history.length - 1) } : {}),
      };
    });
  const pr = {
    title: "Signup conversion tracking",
    repo: "HeardBack / web",
    number: "406",
    branch: "feat/signup-tracking",
    base: "main",
    additions: timeNow >= 73 ? 52 : 46,
    deletions: 8,
    state: timeNow >= 83 ? "Merged" : "Open",
    head: timeNow >= 73 ? "tracking-b2" : "tracking-a1",
    checks:
      timeNow >= 76
        ? "Passed"
        : timeNow >= 73
          ? "Running"
          : timeNow >= 68
            ? "Failed"
            : "Running",
    review: timeNow >= 82 ? "Approved" : "Pending",
    deploy: timeNow >= 76 ? "Staging verified" : "Staging pending",
  };
  return {
    ...scene,
    time: timeNow,
    sceneIndex,
    threads,
    messages: resultMessages,
    draft,
    typing,
    target: pointer.target,
    click: timeNow - pointer.at >= 0.55 && timeNow - pointer.at < 1.05,
    artifact:
      scene.project === "HeardBack"
        ? timeNow >= 104
          ? "schedule"
          : timeNow >= 85
            ? "pack"
            : "copy"
        : null,
    prs: { tracking: pr },
  };
}

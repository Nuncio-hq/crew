// Stable state IDs are agent handoff anchors. This is the only scenario contract.
export const version = "0.8";
export const scenarios = [
  {
    id: "settings",
    label: "Settings · recap runtime",
    screen: "settings",
    status: "idle",
    tools: false,
    intent: "Chọn runtime/profile hoặc model cho recap theo yêu cầu.",
    steps: [
      "Mở Settings và chọn runtime.",
      "Save settings, trở lại Context và Generate recap.",
    ],
    expected: [
      "Không đổi model làm việc của agent.",
      "Plan/activity không cần summarizer.",
      "Không gọi runtime thật trong prototype.",
    ],
    seam: "Global agent settings and runtime discovery; user-facing recap wiring remains proposed.",
  },
  {
    id: "thread-working",
    label: "Thread · working",
    screen: "thread",
    status: "working",
    tools: true,
    intent: "Theo dõi một công việc trong thread, cùng tài liệu liên quan.",
    steps: [
      "Chọn NuncioCrew → Rebuild the workspace.",
      "Mở tab Activity để xem hoạt động mẫu.",
      "Mở Files / Browser; kéo vạch chia để đổi chiều rộng.",
    ],
    expected: [
      "Thread thay thế vùng giữa; không thêm một cột chat hẹp.",
      "Tools chỉ xuất hiện khi mở thread, với tên thread rõ ràng.",
      "Agent plans hiển thị riêng từng agent; vị trí tab chờ G-THREAD-1.",
    ],
    seam: "Channel thread + observer transcript + ChannelToolPane",
  },
  {
    id: "channel",
    label: "Channel · conversation",
    screen: "channel",
    status: "working",
    tools: false,
    intent: "Xem cuộc trao đổi trong channel và đi vào thread.",
    steps: [
      "Chọn # product dưới NuncioCrew.",
      "Lọc Needs you / In progress / Done; mở từng thread.",
      "Đọc role ở channel; participant trong thread kế thừa role. View activity mở đúng transcript; badge PR mở đúng PR.",
      "Watch workspace film: tự chạy marketing → engineering → handoff; playback là câu chuyện mô phỏng.",
      "Bấm Back to channel hoặc Escape.",
    ],
    expected: [
      "Mở cùng thread ở vùng giữa.",
      "Quay lại channel giữ vị trí cuộn.",
      "Chuyển thread không tạo task, session hoặc worktree mới.",
    ],
    seam: "NIP-29 channel + existing thread root/replies",
  },
  {
    id: "needs-you",
    label: "Thread · needs you",
    screen: "thread",
    status: "needs-you",
    tools: true,
    intent:
      "Agent đặt một câu hỏi cần quyết định, ngay trong ngữ cảnh công việc.",
    steps: [
      "Mở Choose the release scope và đọc câu hỏi.",
      "Chọn một phương án và bấm Confirm choice.",
      "Quan sát trạng thái chuyển về Working (mô phỏng).",
    ],
    expected: [
      "Không tự chọn thay founder.",
      "Câu trả lời có phản hồi trong thread.",
      "Không gọi runtime thật.",
    ],
    seam: "Existing pending user-input / permission and observer controls",
  },
  {
    id: "done",
    label: "Thread · ready for review",
    screen: "thread",
    status: "done",
    tools: true,
    intent: "Đọc kết quả và kiểm tra evidence sau khi agent dừng.",
    steps: [
      "Đọc báo cáo hoàn thành mẫu.",
      "Mở PR hoặc Files.",
      "Chọn Activity để xem lịch sử.",
    ],
    expected: [
      "Không còn Stop/Steer cho job đã xong.",
      "Hoàn thành turn không đồng nghĩa founder đã chấp nhận.",
      "Lịch sử và artifacts vẫn đọc được.",
    ],
    seam: "Durable receipt + existing thread evidence surfaces",
  },
  {
    id: "offline",
    label: "Thread · disconnected",
    screen: "thread",
    status: "offline",
    tools: false,
    intent: "Phân biệt mất kết nối với agent đã hoàn thành.",
    steps: [
      "Quan sát cảnh báo và dữ liệu cũ.",
      "Thử gõ một draft.",
      "Bấm Reconnect (mô phỏng).",
    ],
    expected: [
      "Không suy diễn disconnected thành done.",
      "Không gửi khi mất kết nối; draft vẫn còn.",
      "Reconnect trả lại trạng thái và khả năng gửi.",
    ],
    seam: "Relay/observer connection state; no new authority store",
  },
  {
    id: "dm",
    label: "Agent · direct message",
    screen: "dm",
    status: "idle",
    tools: false,
    intent: "Trao đổi riêng với agent, khác với trang quản lý Agents.",
    steps: [
      "Chọn Hermes trong nhóm Direct messages.",
      "Gõ và gửi tin nhắn mẫu.",
      "Chọn Agents ở nhóm đầu để xem directory.",
    ],
    expected: [
      "DM có tên người nhận rõ ràng.",
      "Directory và cuộc trò chuyện riêng là hai bề mặt khác nhau.",
      "Gửi chỉ tồn tại trong bộ nhớ prototype.",
    ],
    seam: "Existing direct-message conversation + managed-agent directory",
  },
  {
    id: "empty",
    label: "Project · empty channel",
    screen: "empty",
    status: "idle",
    tools: false,
    intent: "Bắt đầu cuộc trao đổi mới trong project.",
    steps: ["Chọn HeardBack → # general.", "Viết tin nhắn đầu tiên."],
    expected: [
      "Có composer và hướng dẫn ngắn.",
      "Gửi tạo một tin nhắn local; không tự tạo worktree.",
    ],
    seam: "Existing channel composer; project mapping not finalized",
  },
];
scenarios.push(
  {
    id: "inbox",
    label: "Inbox · attention queue",
    screen: "inbox",
    status: "idle",
    tools: false,
    intent: "Chọn công việc cần quyết định hoặc review.",
    steps: [
      "Lọc Needs you hoặc Ready for review.",
      "Mở một item về đúng thread.",
    ],
    expected: ["Inbox và thread là hai cách xem cùng công việc."],
    seam: "Existing Inbox/feed and thread navigation",
  },
  {
    id: "wiki",
    label: "Wiki · knowledge",
    screen: "wiki",
    status: "idle",
    tools: false,
    intent: "Đọc tài liệu dùng chung.",
    steps: ["Chọn một tài liệu trong Wiki."],
    expected: ["Đổi nội dung ở vùng giữa."],
    seam: "Existing Crew Wiki surface",
  },
  {
    id: "agents",
    label: "Agents · directory",
    screen: "agents",
    status: "idle",
    tools: false,
    intent: "Tìm agent và bắt đầu DM.",
    steps: ["Tìm theo tên hoặc vai trò.", "Bấm Message."],
    expected: ["Directory khác với nhóm Direct messages ở sidebar."],
    seam: "Existing managed-agent directory and DM navigation",
  },
  {
    id: "workflows",
    label: "Workflows · routines",
    screen: "workflows",
    status: "idle",
    tools: false,
    intent: "Xem routine và thử pause/resume mẫu.",
    steps: ["Bấm Pause example rồi Resume example."],
    expected: ["Chỉ thay đổi state local, không tạo lịch thật."],
    seam: "Existing workflow catalog and runtime",
  },
);
export const layoutContract = [
  [
    "L1",
    "Accepted direction",
    "Workspace: Inbox, Wiki, Agents, Workflows và Channels cho channel độc lập/general.",
  ],
  ["L2", "Accepted direction", "Nhóm giữa sidebar: Projects."],
  ["L3", "Accepted direction", "Nhóm dưới sidebar: nhắn tin riêng với agents."],
  ["L4", "Accepted direction", "Vùng giữa: channel, thread hoặc DM đang chọn."],
  [
    "P1",
    "Proposal",
    "Project mở ra channels và recent threads; grouping không đổi danh tính relay.",
  ],
  [
    "P2",
    "Proposal",
    "Tools bên phải có tab; Context/Plan không là cột thứ ba mặc định.",
  ],
  [
    "P3",
    "Proposal",
    "Trạng thái, shortcuts và các flow ở đây cần founder duyệt trước implementation.",
  ],
];
export const agents = [
  {
    name: "Hermes",
    role: "Engineering lead",
    status: "Working",
    icon: "sparkles",
  },
  { name: "Codex", role: "Implementation", status: "Available", icon: "code" },
  {
    name: "Claude",
    role: "Review & research",
    status: "Available",
    icon: "asterisk",
  },
];

scenarios.push({
  id: "completed",
  label: "Thread · accepted outcome",
  screen: "thread",
  status: "completed",
  tools: true,
  intent: "Phân biệt kết quả đã accept và kết quả đang chờ review.",
  steps: [
    "Đọc receipt của Fix unread thread navigation.",
    "Thử Reopen rồi quay lại channel.",
  ],
  expected: [
    "Done có receipt; Reopen chuyển cùng thread về Working.",
    "Không merge hoặc deploy thật.",
  ],
  seam: "Existing thread/task lifecycle; acceptance persistence remains a proposal",
});

scenarios.push(
  ...[
    { id: "blocked", label: "Thread · blocked CI", status: "blocked" },
    {
      id: "discussion",
      label: "Thread · discussion only",
      status: "discussion",
    },
  ].map((s) => ({
    ...s,
    screen: "thread",
    tools: true,
    intent:
      "Đọc trạng thái và lý do mà không suy diễn cần founder hoặc đã hoàn tất.",
    steps: ["Mở Context; trở về channel để đối chiếu trạng thái."],
    expected: [
      "Owner và trạng thái nhất quán với channel.",
      "Không tự chuyển thành Done.",
    ],
    seam: "Existing thread and task lifecycle projection",
  })),
);

// Stage 0 authority matrix. Status applies only to the stated contract, not all mock controls.
export const handoffContract = [
  {
    id: "G-SHELL-1",
    state: "channel, empty, sidebar",
    status: "accepted",
    source: "Founder decision in #344, 2026-09-09; coordinator clarification",
    contract:
      "Conceptual split only: standalone/general channels remain in Channels within Workspace; project channels appear under Projects. All joined channels remain reachable when project metadata is missing/inaccessible. Existing home, related and repository channel relationships; no one-project-only invariant. Detailed UI/interactions remain proposed.",
    owner: "#349",
  },
  {
    id: "SHELL-DETAIL",
    state: "channel, empty, sidebar",
    status: "proposed",
    source: "Stage 0 bounded recommendation; founder discussion ongoing",
    contract:
      "Use Project.projectChannelId, relatedChannelIds / buzz-related-channel and member repository channels; coordinates identify, names label. Detailed grouping, navigation and deduplication require review. Do not change the visual prototype yet.",
    owner: "#349",
  },
  {
    id: "AGENTS",
    state: "agents: Add / Edit / Delete",
    status: "accepted",
    source:
      "Explicit founder requests recorded in prototype AGENTS.md and #344",
    contract:
      "Real lifecycle controls with stable identity/history; preserve unassigned role definitions, runtime installations, profiles and worktrees. Channel cleanup is durable configuration work.",
    owner: "#353",
  },
  {
    id: "ROLES",
    state: "channel, empty: Assign roles",
    status: "accepted",
    source: "Explicit founder requests in #344",
    contract:
      "Unassigned definitions, multiple holders per role, at most one role per agent per channel. Optional member contact is independent of role/thread owner. One atomic roles/contact snapshot; explicit mentions take precedence, including replies. Routing implementation remains gated below.",
    owner: "#350",
  },
  {
    id: "PLANS",
    state: "thread-working: Agent plans",
    status: "accepted",
    source: "Explicit founder requests in #344",
    contract:
      "Separate reported snapshots per agent; preserve empty/retired/unknown/disconnected semantics and owner scope. No synthesized task list. Placement remains G-THREAD-1.",
    owner: "#354",
  },
  {
    id: "SETTINGS",
    state: "settings; Context Generate/Regenerate",
    status: "accepted",
    source: "Explicit founder requests in #344 and prototype AGENTS.md",
    contract:
      "Settings beside footer avatar/name, replacing Personal. Recap Off initially, manual Generate/Regenerate, separate runtime/model/profile settings; never mutate execution-agent model/session. Runtime support requires #351 proof.",
    owner: "#351, #356",
  },
  {
    id: "G-THREAD-1",
    state: "thread-working, done, completed, offline; tools",
    status: "proposed",
    source: "Stage 0 recommendation, not founder approval",
    contract:
      "Agent plans tab supersedes only D-056 permanent placement; keep per-agent source/ordering. Historical transcripts readable in same thread, exact-live-job controls only; no Workbench picker. Context initially; selected tool per exact thread within session; narrow overlay keeps conversation readable.",
    owner: "#354",
  },
  {
    id: "G-RECAP-1",
    state: "settings; Context recap",
    status: "proposed",
    source: "Stage 0 recommendation, runtime capabilities unverified",
    contract:
      "Owner-local derived cache in Context; no automatic channel post or instruction/acceptance authority. Capability, tool restriction, profile, provenance and watermark need certified runtime evidence.",
    owner: "#351, #356",
  },
  {
    id: "G-CONTACT-1",
    state: "channel, replies: unmentioned human send",
    status: "blocked",
    source: "Existing receipt authority; #355 proof gate",
    contract:
      "Relay-authoritative unique routing, durable replay/execution and compatibility proof, visible response/no bare acknowledgement, then approved implementation successor. Current receipt validation rejects mentionless direct triggers; client canvas selection cannot authorize them.",
    owner: "#355 + approved successor",
  },
  {
    id: "G-ACCEPTANCE-1",
    state: "done, completed: Accept/Reopen",
    status: "blocked",
    source: "D-070 and #344 preserve current founder review ritual",
    contract:
      "Founder-authored acceptance names exact outcome revision/commit and evidence. Mock buttons grant no protocol authority. Durable controls require decision on authority, revision, evidence manifest, duplicate/race/reopen semantics and existing Buzz seam before a separate implementation issue.",
    owner: "#344",
  },
  {
    id: "G-LATENCY-1",
    state: "thread-working: lifecycle activity",
    status: "proposed",
    source: "D-075 / #344 bounded ordering recommendation",
    contract:
      "Preserve channel ordering, null barriers, quota/bytes/drop accounting. Any stronger target requiring within-channel reorder/coalescing needs explicit lead decision.",
    owner: "#352",
  },
  {
    id: "RUNTIME-ACCEPTANCE",
    state: "all visible controls",
    status: "blocked",
    source: "Founder execution/release contract in #344",
    contract:
      "Staging proof plus final installed release on designated company dev-server test channels: real agents, actual handler effects, persisted readback and recovery. Coordinator and Fable approve exact final PR head; no executor merge/release.",
    owner: "#348, #357 and dependent issues",
  },
];

// Stable state IDs are agent handoff anchors. This is the only scenario contract.
export const version = "0.9";
export const scenarios = [
  ...[
    "empty",
    "missing",
    "updating",
    "failed",
    "stale",
    "unavailable",
    "answer-failed",
  ].map((state) => ({
    id: `wiki-${state}`,
    label: `Wiki · ${state}`,
    screen: "wiki",
    status: "idle",
    tools: false,
    intent: "Review Wiki recovery and access states.",
    steps: ["Read the state message and use its recovery action."],
    expected: [
      "Cached pages remain readable; generation and answers are simulated.",
    ],
    seam: "Existing Wiki events/job state; private QA and task handoff require real wiring.",
  })),
  {
    id: "project",
    label: "Project · channels & workspace",
    screen: "project",
    status: "idle",
    tools: false,
    intent:
      "Project page nhẹ: Channels là nơi làm việc, Workspace quản lý folder.",
    steps: [
      "Bấm tên Project hoặc breadcrumb NuncioCrew.",
      "Mở channel; Add channel hoặc Link existing.",
      "Add project: nhập tên, thử folder Git / thường / không truy cập được, tạo hoặc nối channel.",
      "Manage workspace: đổi folder hoặc unlink; Cancel/Escape giữ nguyên.",
    ],
    expected: [
      "Mũi tên chỉ bung/thu channels; tên mở Project page.",
      "Folder tùy chọn; Git detection và tạo project chỉ là mô phỏng.",
      "Không sửa folder, Git, relay hoặc phiên agent thật.",
      "Luồng Project v0.9 đã được founder duyệt trong #344; backend semantics còn các gate cụ thể.",
    ],
    seam: "Existing Project + Repository.localWorkspacePath, projectRelatedChannels, native folder picker/Git probe. Unifying legacy folder creation with explicit Project remains implementation work.",
  },
  {
    id: "channels",
    label: "Workspace · shared channels",
    screen: "empty",
    status: "idle",
    tools: false,
    intent: "Giữ channel chung trong Workspace.",
    steps: [
      "Mở menu workspace → Browse channels để vào channel chung.",
      "Menu workspace → Browse channels vẫn mở được các channel chung đã liên kết Project.",
      "Mở một channel hoặc liên kết nó vào Project qua Add channel.",
    ],
    expected: [
      "Các channel chung vẫn truy cập được; liên kết project không sao chép lịch sử.",
      "Tất cả thay đổi chỉ nằm trong bộ nhớ prototype.",
    ],
    seam: "Existing joined NIP-29 channels and Project related channels.",
  },
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
    label: "Wiki · read, ask & task",
    screen: "wiki",
    status: "idle",
    tools: false,
    intent: "Wiki theo Project: đọc, hỏi, kiểm tra nguồn và tạo draft.",
    steps: [
      "Project → Wiki; đọc bài, tìm trong toàn văn.",
      "Ask → hỏi, mở citation và đóng source.",
      "Create task draft → chọn channel/agent → Start thread → Back to Wiki.",
    ],
    expected: [
      "Ask thay bài đọc; mở nguồn ẩn TOC.",
      "Lịch sử riêng, chỉ Start thread chia sẻ draft đã duyệt.",
      "Agent/generation và việc tạo task là mô phỏng.",
    ],
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
    "Nhóm đầu: Inbox, Agents, Workflows. Wiki nằm trong mỗi Project.",
  ],
  ["L2", "Accepted direction", "Nhóm giữa sidebar: Projects."],
  ["L3", "Accepted direction", "Nhóm dưới sidebar: nhắn tin riêng với agents."],
  ["L4", "Accepted direction", "Vùng giữa: channel, thread hoặc DM đang chọn."],
  [
    "L5",
    "Accepted direction",
    "Project name/breadcrumb mở Channels/Workspace; chevron chỉ bung/thu Wiki và channels. Browse channels trong workspace menu.",
  ],
  [
    "P2",
    "Proposal",
    "Tools bên phải có tab; Context/Plan không là cột thứ ba mặc định.",
  ],
  [
    "P3",
    "Proposal",
    "Luồng Project/Wiki v0.9 đã được duyệt; các gate backend và đề xuất khác nằm trong Stage 0 matrix.",
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

// Authority applies only to the stated contract, never every sample control.
export const handoffContract = [
  {
    id: "G-SHELL-1",
    state: "sidebar, project, channels, wiki",
    status: "accepted",
    source:
      "Founder messages after Wiki v0.9 walkthrough, 2026-09-09, recorded in #344: “tốt rồim, tạo issues mới đi”; “wiki và cả project nhé nếu chưa tạo”; subsequent implementation instruction.",
    contract:
      "Workspace top: Inbox, Agents, Workflows. Wiki is inside each Project; no global Wiki or Channels rows. Workspace menu → Browse channels stays reachable even if every shared channel is linked to a Project. Preserve all joined-channel reachability when project metadata is missing/inaccessible, membership/history/roles/contact scope and many-to-many relationships. Company handbook replacement is the separate coordinator proposal below.",
    owner: "#349",
  },
  {
    id: "PROJECT-FLOW",
    state:
      "project: name/breadcrumb, expand, Add project, Add channel, Link folder, Manage/Unlink",
    status: "accepted",
    source:
      "Founder messages after Wiki v0.9 walkthrough, 2026-09-09, recorded in #344: “tốt rồim, tạo issues mới đi”; “wiki và cả project nhé nếu chưa tạo”; subsequent implementation instruction.",
    contract:
      "Demonstrated lightweight Channels/Workspace page; name/breadcrumb opens it and chevron only expands. Optional folder and new/existing main channel; create/link related channel; folder link/manage/unlink with Cancel. Existing 30621 Project and exact selected 30617 Repository remain authoritative. Backend model/host/path/recovery choices remain gated by G-PROJECT and G-DURABLE.",
    owner: "#349, #361",
  },
  {
    id: "WIKI-READ",
    state: "wiki: Read, Search, Source, Contents; wiki-empty/missing/stale",
    status: "accepted",
    source:
      "Founder messages after Wiki v0.9 walkthrough, 2026-09-09, recorded in #344: “tốt rồim, tạo issues mới đi”; “wiki và cả project nhé nếu chưa tạo”; subsequent implementation instruction.",
    contract:
      "Project-scoped TOC/article; full-body search with excerpts; immutable cited source hides TOC. Ask replaces article. Remember page/scroll; one repository automatic, multiple require stable selection. Narrow Contents and dismissible source retain recovery/focus. Preserve company knowledge; no new CMS or implicit generation.",
    owner: "#364",
  },
  {
    id: "WIKI-GENERATE",
    state:
      "wiki-empty/updating/failed/stale: Generate/Update, Cancel, Retry, runtime settings",
    status: "accepted",
    source:
      "Founder messages after Wiki v0.9 walkthrough, 2026-09-09, recorded in #344: “tốt rồim, tạo issues mới đi”; “wiki và cả project nhé nếu chưa tạo”; subsequent implementation instruction.",
    contract:
      "Temporary selected installed-runtime/profile generation independent of Recap and employee sessions. Old complete snapshot remains readable on failure/cancel. Runtime capabilities, immutable Git/folder snapshot and coherent publication need G-GEN/G-PUB proof. Sample timers are not backend evidence.",
    owner: "#362, #363",
  },
  {
    id: "WIKI-ASK-DRAFT",
    state:
      "wiki, wiki-unavailable, wiki-answer-failed: Ask, History, Source, draft, Start thread, Back to Wiki",
    status: "accepted",
    source:
      "Founder messages after Wiki v0.9 walkthrough, 2026-09-09, recorded in #344: “tốt rồim, tạo issues mới đi”; “wiki và cả project nhé nếu chưa tạo”; subsequent implementation instruction.",
    contract:
      "Existing selected available Project agent; private question/history by default; asking neither generates nor posts. Editable private task draft; only explicit Start shares reviewed prompt/citations with selected channel/member agent. Exact-question return obeys private-history ACL. Privacy/session/capability and durable dispatch remain gated below.",
    owner: "#365, #366, #367",
  },
  {
    id: "G-HANDBOOK",
    state:
      "workspace menu → Company Wiki → company library (company-wiki preview)",
    status: "accepted",
    source:
      "Coordinator reviewed and approved the concrete compatibility diff, 2026-09-09; not a new founder approval.",
    contract:
      "Add a workspace-menu Company Wiki entry that reuses existing production goWiki() → /wiki → WikiLibraryScreen and its Company Wiki card for kind 30023 knowledge. Keep current content/ACL/route; no migration or separate store. This source renders only a labeled sample compatibility preview. Coordinator approved this concrete entry; #349 must verify the production replacement before removing the only global Wiki affordance.",
    owner: "#344 review; #349 wiring; #364 compatibility",
  },
  {
    id: "G-PROJECT",
    state: "project: create/link/manage/unlink",
    status: "blocked",
    source: "#361 source audit and model gate",
    contract:
      "Resolve exact legacy/zero/one/multiple Repository mapping, optional folder, owner+d targeting, current-device path probe and live-session replacement policy. No name-based migration, remote-host assumption, or deleting a pre-existing channel on ambiguous writes.",
    owner: "#361",
  },
  {
    id: "G-PUB",
    state: "wiki: update/cancel/retry/current snapshot",
    status: "blocked",
    source: "#362 coherent publication gate",
    contract:
      "Prove generation grammar, immutable page/manifest references, relay retention and legacy compatibility before schema-dependent publication. Old complete snapshot must survive partial publish/restart/second-client reads; no fabricated fallback success.",
    owner: "#362",
  },
  {
    id: "G-DURABLE",
    state:
      "Project operations; Wiki publication, private history and task draft/dispatch",
    status: "blocked",
    source: "#362 shared architecture gate",
    contract:
      "Name and approve a bounded owner-local durable operation/history seam; specialized theme/observer stores are not generic outboxes. Separate private history from signed shared-publication recovery; define ACL/encryption, limits, atomic persistence, migration and retry identities. Buzz events remain authoritative.",
    owner: "#362 shared; #361, #366, #367 consumers",
  },
  {
    id: "G-GEN",
    state: "wiki: installed-runtime generation and immutable source",
    status: "blocked",
    source: "#363 runtime/snapshot gate",
    contract:
      "Certify temporary runtime/profile/model invocation, read-only/no-tools or bounded snapshot tools, containment/cancel and immutable Git/folder source. No active-employee session reuse, dirty bytes labeled HEAD, or silent API/heuristic fallback.",
    owner: "#363",
  },
  {
    id: "G-ASK-PROOF",
    state: "wiki: private existing-agent Ask, History and draft origin",
    status: "blocked",
    source: "#365 proof before #366 / #367 shipping",
    contract:
      "Prove identity/ACL/privacy with two authenticated users, isolation from busy employee sessions, no channel/file/task side effect, bounded attempt-scoped stream/cancel/recovery and a certified runtime. Mock private state or prompt obedience is insufficient.",
    owner: "#365; #366, #367",
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
      "For each implemented issue, launch the actual candidate with designated real data and isolated authorized runtime/profile state; exercise handlers/readback/recovery and post safe immutable screenshots plus exact SHA/runtime/data identity on that issue. Related issues may share one unchanged build/run while retaining issue-to-case mapping. Prototype screenshots satisfy only Stage 0 reference evidence. Coordinator + actual Fable Medium must approve exact PR head before merge; #357 retains installed release acceptance.",
    owner: "#348, #357; #349–#356 and #361–#367",
  },
];

scenarios.push({
  id: "company-wiki",
  label: "Company Wiki · compatibility proposal",
  screen: "company-wiki",
  status: "idle",
  tools: false,
  intent: "Keep existing company knowledge reachable from the workspace menu.",
  steps: [
    "Open workspace menu → Company Wiki.",
    "Open the sample company handbook, then return to its library.",
  ],
  expected: [
    "Coordinator-proposed compatibility entry; not founder-approved placement.",
    "Production wiring reuses goWiki() /wiki and WikiLibraryScreen; preview stays simulated.",
  ],
  seam: "Existing WikiLibraryScreen Company Wiki card and kind 30023 pages; #349 owns production wiring after coordinator review.",
});

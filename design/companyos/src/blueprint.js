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
      "Chi tiết UI đang chờ founder review; chưa là hợp đồng production.",
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
      "Plan nằm trong Activity/Context, không chiếm thêm cột cố định.",
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

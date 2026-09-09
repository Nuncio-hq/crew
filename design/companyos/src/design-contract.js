// Explanatory design intent. State behavior lives only in blueprint.js.
// Accepted v0.9 Project/Wiki flow and remaining technical gates are in blueprint.js.
export const sections = [
  {
    id: "inbox",
    number: "01",
    title: "Inbox",
    zone: "Sidebar · nhóm 1",
    state: "inbox",
    purpose:
      "Nơi chọn việc cần chú ý, trước khi đi vào cuộc hội thoại liên quan.",
    shows:
      "Câu hỏi cần founder trả lời, kết quả cần review và ngữ cảnh project/thread của từng mục.",
    actions:
      "Lọc Needs you hoặc Ready for review; mở item vào chính thread gốc ở vùng giữa.",
    boundary:
      "Không phải một channel mới, không sao chép hội thoại và không tự tạo task khi người dùng mở item.",
    open: "Quy tắc gom nhóm, unread, snooze và độ ưu tiên chưa chốt.",
    seam: "Inbox/feed hiện có + điều hướng theo thread root.",
  },
  {
    id: "wiki",
    number: "02",
    title: "Wiki",
    zone: "Sidebar · trong mỗi Project",
    state: "wiki",
    purpose:
      "Đọc và hỏi về repository ngay trong project, rồi chuyển hiểu biết thành bản nháp công việc.",
    shows:
      "TOC + bài đọc; Ask thay bài ở vùng giữa. Mở nguồn thì ẩn TOC, hiện code tại revision và dòng được dẫn. Một repository được chọn tự động; nhiều repository mới cần picker.",
    actions:
      "Tìm toàn văn → đọc → hỏi agent hiện có → xem nguồn → Create task draft → chọn channel/agent và sửa prompt → Start thread. Lịch sử riêng và bản nháp giữ trong phiên preview; thread có Back to Wiki.",
    boundary:
      "Hỏi không tự tạo task, không đăng channel và không chạy lại generator. Chỉ Start thread chia sẻ prompt + references đã duyệt. Runtime Wiki là phiên tạm riêng, độc lập Recap; Hermes chọn profile.",
    open: "Prototype dùng bài mẫu và câu trả lời chuẩn bị trước; không có LLM/indexing thật. Company handbook không bị xóa; menu → Company Wiki là compatibility proposal của coordinator, chờ review concrete diff. Private-history ACL/persistence và dispatch thật chưa được nối.",
    seam: "Tái dùng crew-wiki, WikiPageView/WikiTocRail/WikiSourceFiles, wikiEvents và useWikiEventsQuery. Không tạo knowledge database song song. wikiAsk hiện là placeholder: cần QA với source snapshot, citations, cancel/retry và handoff vào channel/thread hiện hữu.",
  },
  {
    id: "agents",
    number: "03",
    title: "Agents",
    zone: "Sidebar · nhóm 1",
    state: "agents",
    purpose:
      "Danh mục nhân sự agent: tìm đúng người cho công việc và xem vai trò của họ.",
    shows:
      "Tên, runtime, profile hoặc model, channels và trạng thái được biết. Role nằm ở từng channel, không là thuộc tính toàn cục.",
    actions:
      "Add agent: tên, runtime, Hermes profile hoặc model ID cho Codex/Claude, channels tùy chọn. Edit sửa cấu hình, Cancel bỏ draft; Message dùng identity ổn định dù tên đổi.",
    boundary:
      "Trang Agents là directory; nhóm Direct messages là các cuộc hội thoại. Available không đồng nghĩa agent được phép tự nhận mọi việc.",
    open: "Profile discovery, model availability, config acknowledgement và runtime lifecycle cần nối nguồn thật. Đổi cấu hình không được ngầm đổi phiên đang chạy.",
    seam: "Managed-agent directory và hồ sơ hiện có.",
  },
  {
    id: "workflows",
    number: "04",
    title: "Workflows",
    zone: "Sidebar · nhóm 1",
    state: "workflows",
    purpose: "Xem những quy trình lặp lại đã được cấu hình cho workspace.",
    shows:
      "Tên quy trình, mục đích, lịch hoặc trigger, và trạng thái hoạt động.",
    actions:
      "Bản mẫu cho thử Pause/Resume một workflow local để thảo luận cách phản hồi trạng thái.",
    boundary:
      "Không coi ví dụ trong UI là lịch đã được cài. Không tự bật automation hoặc nới quyền của agent.",
    open: "Tạo/sửa workflow, lịch sử lần chạy và xử lý lỗi chưa nằm trong prototype v0.1.",
    seam: "Workflow catalog và engine hiện có.",
  },
  {
    id: "projects",
    number: "05",
    title: "Projects",
    zone: "Sidebar · nhóm 2",
    state: "project",
    purpose:
      "Một Project gom channels và workspace; channel/thread vẫn là nơi làm việc hằng ngày.",
    shows:
      "Trang Project nhẹ gồm Channels và Workspace. Folder cho biết máy chứa, đường dẫn và Git/folder mode; không sao chép dữ liệu qua relay.",
    actions:
      "Tên Project hoặc breadcrumb mở trang Project; mũi tên chỉ bung/thu channels. Không còn global Channels/Wiki row; Browse channels nằm trong menu workspace kể cả khi mọi channel chung đã liên kết Project. Add project nhập tên, folder tùy chọn, main channel mới/có sẵn. Add channel, Link folder, Manage và Unlink dùng dữ liệu mẫu trong bộ nhớ.",
    boundary:
      "Founder đã duyệt demonstrated Project/Wiki v0.9 flow và yêu cầu implementation; provenance trong #344. Folder picker, Git detection và việc tạo/liên kết chỉ mô phỏng. Không gọi native API, sửa Git, folder, relay hay active agent.",
    open: "G-PROJECT (#361) còn phải chứng minh mapping folder legacy Repository với explicit Project, zero/multiple repositories và exact owner+d writes. Đường dẫn chính xác, truy cập theo host, retry publish/readback và channel membership cần nguồn thật. Folder không truy cập được phải có trạng thái riêng.",
    seam: "Project (30621), Repository (30617) + localWorkspacePath, projectRelatedChannels; existing native folder picker and probeProjectGitWorkspace.",
  },
  {
    id: "direct-messages",
    number: "06",
    title: "Direct messages",
    zone: "Sidebar · nhóm 3",
    state: "dm",
    purpose: "Lối vào nhanh cho cuộc trao đổi riêng với một agent cụ thể.",
    shows: "Tên người nhận, cuộc hội thoại đang chọn và trạng thái được biết.",
    actions:
      "Chọn agent; đọc lịch sử; gửi câu hỏi riêng. Prototype trả lời bằng dữ liệu mô phỏng.",
    boundary:
      "Không dùng tên runtime làm danh tính hội thoại. DM không tự chuyển thành task của project, cũng không cấp thêm quyền.",
    open: "Quy tắc đưa một trao đổi riêng sang project/thread và group DM chưa được thiết kế.",
    seam: "Direct-message conversation và danh tính agent hiện có.",
  },
  {
    id: "channel",
    number: "07",
    title: "Channel",
    zone: "Vùng giữa · thảo luận chung",
    state: "channel",
    purpose:
      "Xem bức tranh chung và các cuộc trao đổi của một nhóm trong project.",
    shows:
      "Tên channel, nhiều thread với trạng thái, owner, activity trực tiếp, attention và badge riêng từng PR/CI; role theo channel và người tham gia thread; composer cho channel.",
    actions:
      "Đọc thảo luận; mở replies để đi vào thread; gửi tin nhắn tới đúng channel. Assign roles có Add role, tên/định nghĩa, nhiều người giữ role và Unassigned. Chọn agent chuyển assignment khỏi role cũ trong cùng channel. Channel contact point nhận tin người dùng không @mention, kể cả replies; mention luôn ưu tiên. Save roles và contact cùng một snapshot; Cancel bỏ draft.",
    boundary:
      "Channel và thread không dùng chung đích gửi mặc định. Mở thread không tự tạo session, task hoặc worktree.",
    open: "Bộ lọc attention đang là đề xuất; retention, quy mô lớn và multi-task aggregation chưa chốt.",
    seam: "NIP-29 h-tag scope, message root/replies và channel composer.",
  },
  {
    id: "thread",
    number: "08",
    title: "Thread / Focus",
    zone: "Vùng giữa · công việc cụ thể",
    state: "thread-working",
    purpose:
      "Đọc, theo dõi và trao đổi về cùng một công việc mà vẫn có đủ chỗ để hiểu nội dung.",
    shows:
      "Tiêu đề dễ đọc, conversation/activity, người phụ trách và trạng thái. Công cụ liên quan nằm bên phải khi cần.",
    actions:
      "Chuyển Conversation/Activity; trả lời; Steer/Stop khi có job live; trở về channel giữ vị trí.",
    boundary:
      "Không thêm một Workbench picker độc lập. Thread đã kết thúc vẫn đọc được, nhưng không hiện Stop cho một job không còn chạy.",
    open: "Chốt cách truy cập lịch sử observer và ranh giới đọc thread/điều khiển job theo D-065 trước khi implement.",
    seam: "Thread root, observer transcript, pending user-input và điều khiển job hiện có.",
  },
  {
    id: "tools",
    number: "09",
    title: "Tools & context",
    zone: "Vùng phải · mở theo nhu cầu",
    state: "thread-working",
    purpose:
      "Đặt kết quả và ngữ cảnh công việc cạnh cuộc hội thoại, để người dùng vừa đọc vừa kiểm tra.",
    shows:
      "Tabs Files, Browser, PRs, Actions, Terminal, Context trong bản đề xuất. Agent plans là tab riêng trong prototype; placement chờ G-THREAD-1.",
    actions:
      "Đổi tab; đóng/mở; kéo vạch chia hoặc dùng phím mũi tên. Cửa sổ hẹp mở tools thành vùng có thể đóng.",
    boundary:
      "Không coi terminal log, PR diff hay browser mẫu là kết quả thật. Đổi tab không chạy lệnh, merge PR hoặc start server.",
    open: "Simulator, default tab, restore trạng thái theo thread và kích thước cuối cùng còn cần duyệt.",
    seam: "Tái sử dụng ChannelToolPane và các presenter hiện có.",
  },
  {
    id: "composer",
    number: "10",
    title: "Composer & job controls",
    zone: "Dưới cuộc hội thoại đang chọn",
    state: "offline",
    purpose:
      "Gửi đúng nội dung tới đúng nơi, đồng thời giữ được draft và ý định khi trạng thái thay đổi.",
    shows:
      "Đích channel/thread/DM, draft, nút gửi và điều khiển job khi phù hợp.",
    actions:
      "Enter gửi; Shift+Enter xuống dòng. Mất kết nối giữ draft và chặn gửi; Reconnect mẫu phục hồi khả năng gửi.",
    boundary:
      "Không suy diễn draft là lệnh đã gửi. Stop không xóa thread. Steer không tạo job thứ hai một cách ngầm định.",
    open: "Attachment, nhắm nhiều agent, delivery/ack, retry thật và quyền Stop/Steer chưa chốt.",
    seam: "Existing composer/send, connection state và observer controls.",
  },
];
export const vocabulary = [
  [
    "Project",
    "Nhóm công việc hiển thị cho người dùng; mapping kỹ thuật còn mở.",
    "Không đồng nhất với repository hoặc folder.",
  ],
  [
    "Channel",
    "Không gian trao đổi chung, có thành viên và phạm vi relay.",
    "Không tự tạo channel cho mọi khách hàng hoặc department.",
  ],
  [
    "Thread",
    "Chuỗi hội thoại bắt nguồn từ một message root.",
    "Không đồng nhất với task, runtime session hay worktree.",
  ],
  [
    "Task / job",
    "Đơn vị công việc và lần thực thi cần trạng thái, owner rõ ràng.",
    "Xem một thread không có nghĩa bắt đầu job mới.",
  ],
  [
    "Session",
    "Ngữ cảnh chạy của runtime.",
    "Không dùng lifecycle session để suy diễn founder acceptance.",
  ],
  [
    "Worktree",
    "Checkout Git phục vụ công việc khi cần.",
    "Không tạo chỉ vì mở một màn hình.",
  ],
  [
    "Ready for review → Done",
    "Ready for review là kết quả chờ duyệt; Done là scope và revision đã được chấp nhận.",
    "Turn kết thúc hoặc CI xanh riêng lẻ không đủ. Xem hợp đồng Thread, attention & hoàn tất.",
  ],
  [
    "Disconnected",
    "Không thể xác nhận cập nhật mới.",
    "Không chuyển sang Done hoặc Available do thiếu tín hiệu.",
  ],
];

// Code-inspected feasibility, not claims of end-to-end execution.
export const feasibility = [
  {
    title: "Layout, Projects và điều hướng",
    status: "Frontend và backend nền tảng đã có; layout mới chưa nối",
    source:
      "AppShell / AppSidebar, ChannelPane / MessageThreadPanel; NIP-MP kind 30621 nhóm repositories 30617; Project.projectChannelId, relatedChannelIds / buzz-related-channel và repository channels đã có.",
    remaining:
      "Đưa layout D-078 vào app hiện có; reuse existing home/related/repository channel relationships; no one-project-only invariant. Sửa guardrail check-channel-first-ia đang cấm Projects; giữ cấm Workbench picker. Không tạo registry React song song.",
    limit:
      "D-056 còn yêu cầu plan rail luôn hiện, prototype dùng tab; cần ghi rõ quyết định thay thế khi chuyển UI. #344 là roadmap; G-THREAD-1 còn mở, Workbench routes vẫn redirect.",
  },
  {
    title: "Channel, thread, DM và search",
    status: "Đã có cả backend và frontend",
    source:
      "NIP-29 / NIP-10, ChannelPane, MessageThreadPanel, composer và search hiện hành.",
    remaining:
      "Tái bố trí và bind các hook/store; giữ mention identity, drafts, unread, attachments, edit, lỗi mạng và keyboard.",
    limit:
      "Prototype chỉ có sample messages. #346 có lỗi composer cần phân loại trước khi dùng E2E làm chuẩn.",
  },
  {
    title: "Inbox và completion",
    status: "Có projection; full delivery loop chưa được chứng minh",
    source:
      "missionInbox.ts, agentReceiptStore.ts, request/receipt 46040..46043 và owner reactions.",
    remaining:
      "Dùng đúng receipt/reaction/PR revision để phân loại Needs you, Ready for review và Accepted; test cold reload và hai client.",
    limit:
      "Receipt reviewed, agent turn done, PR merged và accepted deliverable không đồng nghĩa. #102/#151 đóng nhưng là plan/discussion, không phải orchestration đã ship.",
  },
  {
    title: "Live activity và plans từng agent",
    status: "Đã có backend, projection và UI thật",
    source:
      "observerRelayStore + agentSessionTranscript / conversationActivityHeadline; useDeclaredPlansForThread → ThreadPanelDeclaredPlansBody → DeclaredPlansRail. #190 đã đóng.",
    remaining:
      "Chuyển presenter vào layout mới, giữ owner scope, agent/conversation/session và empty/retired snapshot semantics. Re-measure latency trong #352.",
    limit:
      "Queue hiện pack nhiều events cùng channel vào một frame mỗi tick; không dùng chẩn đoán một event/tick cũ. Chưa đo latency live lần này.",
  },
  {
    title: "Agent Add / Edit / Delete và runtime model/profile",
    status: "Đã có cả backend và frontend",
    source:
      "agents/hooks.ts create/update; useManagedAgentActions → deleteManagedAgentWithRules → delete_managed_agent; Hermes profile discovery, runtime catalog và ModelPicker.",
    remaining:
      "Tái dùng lifecycle đầy đủ trong UI mới; giữ channel-join acknowledgement, remote shutdown/orphan guard, identity history và cleanup retry. Membership signals/badges đã có; #337 là readiness/recovery delta.",
    limit:
      "Không thay bằng xóa một record trong React. Deleting an agent không có nghĩa uninstall runtime/profile/worktree.",
  },
  {
    title: "Assign roles và Channel contact point",
    status: "Roles đã có; contact point là backend mới",
    source:
      "ChannelCanvas đã có Assign role; canvas.rs assign_channel_agent_role / get_canvas / set_canvas; buzz-core crew_role.rs. ACP SubscriptionRule hỗ trợ require_mention và relay #h/#p.",
    remaining:
      "Editor bulk definitions/holders + một optional contact pubkey trong canvas. Conflict-aware save, preserve unrelated keys, contact-aware subscriptions/dispatch, explicit mention priority, verified-human-only fallback và deletion cleanup.",
    limit:
      "set_canvas không có expected revision. Canvas publish và announcement là hai writes: có thể partial success. Tắt require_mention không đủ để chặn bot loops hoặc tin đang gọi người khác. receipt_parent_targets_agent (buzz-relay/src/handlers/ingest.rs) từ chối mentionless direct trigger; #355 phải chứng minh relay authority trước approved implementation successor.",
  },
  {
    title: "Recap và Settings",
    status: "Settings/discovery có; recap theo runtime còn mới",
    source:
      "GlobalAgentConfig + runtime discovery. guided_handover.rs dùng API key, chat/completions rồi invalidate session.",
    remaining:
      "Thêm executor recap qua runtime/profile được chọn, bounded read-only context, cancel/timeout/retry, model/auth validation, source watermark và lưu kết quả.",
    limit:
      "Không gọi guided_handover trực tiếp: nó reset session và không dùng CLI subscription như lựa chọn trong prototype. Activity/plans không cần model phụ.",
  },
  {
    title: "Git, PR, CI và worktree",
    status: "Đã có backend và frontend",
    source:
      "thread_github.rs / thread_forge, ThreadPrHub, ProjectThreadWorkspacePanel, workspace binding/registry. #187/#193 đã đóng.",
    remaining:
      "Gắn đúng thread → checkout → repository → PR/head; giữ local vs HEAD và PR base diff riêng; hiển thị unavailable và removed checkout.",
    limit:
      "CI xanh không chứng minh acceptance/deployment. Thao tác remote cần dùng auth và conflict handling sẵn có.",
  },
  {
    title: "Files, Browser, Simulator và Terminal",
    status: "Phần lớn có sẵn; cần nối đúng pane/context",
    source:
      "ChannelToolPane có PR/Browser/Sim; TerminalBootstrap đã mount trong AppShell và terminal_runtime.rs có PTY thật. Repo file preview có command hiện hành. #196/#197 đã đóng.",
    remaining:
      "Dùng lại terminal substrate/session lifecycle, BrowserTab/SimTab và file-reader seam; bind đúng cwd/thread. File editing mới phải có write/conflict contract riêng nếu đưa vào scope.",
    limit:
      "Không cần xây PTY/browser mới. Prototype Terminal là text mẫu, Files không phải editor thật; local/native/remote availability phải kiểm tra riêng.",
  },
  {
    title: "Wiki và Workflows",
    status: "Đã có engines/UI; không phải mọi action đã hoàn chỉnh",
    source:
      "crew-wiki + WikiLibraryScreen; buzz-workflow và workflow UI. #200 và #274 đã đóng.",
    remaining:
      "Nối Wiki vào Project/Repository hiện có. Thêm full-content search, QA có source references, lịch sử riêng theo project/repo, cancel/retry và draft-to-thread có backlink; kiểm tra quyền đọc repo và channel riêng. Tái dùng generation/job/progress, snapshot và publish của crew-wiki. Routines cần persist enabled/schedule/runs và failure recovery.",
    limit:
      "Wiki read/ask/source/task trong prototype là dữ liệu và timer mô phỏng; source snippets được chụp từ revision 8278d2b. Missing workspace vẫn đọc cached snapshot, failed update không xóa bài; agent thiếu quyền/unavailable không dispatch. Cần xác minh end-to-end bằng relay/runtime thật. Workflow approval gate hiện trả approval_not_supported trong buzz-workflow/src/lib.rs:230. Không hứa tự pause/resume tại approval.",
  },
  {
    title: "Marketing, email, lịch, khách hàng và deadline",
    status: "Nền runtime có; tích hợp cụ thể chưa được chứng minh",
    source:
      "Agents có tools/profile + channel/thread/workflows làm nền non-code.",
    remaining:
      "Chọn connector/provider và quyền theo từng việc, event intake, idempotency, retry, destination/evidence. Là roadmap sau coding loop nếu chưa chốt scope.",
    limit:
      "Film marketing không chứng minh email/social/calendar đã kết nối; không tạo issue riêng cho từng ô mock.",
  },
  {
    title: "Staging và runtime acceptance",
    status: "Policy đã có; chưa có bằng chứng staging sẵn sàng",
    source:
      "D-077 / docs/crew/TESTING.md; #338 AUTH/reconnect, #337 zero-channel readiness; PR #347 chỉ docs.",
    remaining:
      "Provision snapshot staging tách writable data rồi chạy Hermes + runtime thứ hai qua create/join/mention/stream/plan/receipt/reconnect và founder acceptance.",
    limit:
      "Không dùng daily relay làm fallback. #338 là báo cáo mở từ 08-09, chưa retest live hôm nay; NIP-11 trả lời không chứng minh harness AUTH hoạt động.",
  },
];

export const backlogAudit = [
  {
    title: "#344 — v0.9 roadmap and source handoff",
    url: "https://github.com/Nuncio-hq/crew/issues/344",
    action:
      "Roadmap stays open. #349 shell; #350 roles; #351 runtime proof; #352 latency; #353 directory; #354 activity/plans/tools; #355 contact proof + successor; #356 recap; #348 staging; #357 installed acceptance. Project/Wiki: #361 create/link/manage, #362 publication/durable gate, #363 generator, #364 Read/Search/Source, #365 private Ask proof, #366 Ask/history, #367 draft/dispatch.",
  },
  {
    title: "#350 / #355 — roles editor and contact proof",
    action:
      "Một vertical slice gồm canvas bulk save/conflict recovery, contact dispatch/mention priority/human-only fallback, memberships và delete cleanup. Roles nền đã ship ở #116; issue mới chỉ làm phần delta, không viết lại hệ role.",
  },
  {
    title: "#351 / #356 — certified runtime and recap",
    action:
      "Settings + runtime executor + source scope/freshness + cancel/retry/provenance là một feature, không tách issue riêng cho dropdown, model field hay Generate button.",
  },
  {
    title: "#348 / #357 — staging and installed acceptance",
    action:
      "Môi trường isolated snapshot và bộ test workflow thật qua hai runtimes, restart/reconnect, đúng worktree, receipt và founder acceptance. Liên kết #337/#338; không gộp vận hành daily relay vào việc tạo staging.",
  },
  {
    title: "#337 và #338 — giữ riêng, liên kết dependency",
    url: "https://github.com/Nuncio-hq/crew/issues/338",
    action:
      "#338 sửa connection/auth/recovery; #337 sửa trạng thái zero-channel và re-verify membership. AUTH tốt không đảm bảo membership. Socket đóng sau challenge cũng chưa chứng minh credential bị từ chối: cần phân biệt explicit auth failure và transport close.",
  },
  {
    title: "#346 — giữ issue regression/E2E",
    url: "https://github.com/Nuncio-hq/crew/issues/346",
    action:
      "Dùng làm baseline regression khi đổi UI; không chuyển lỗi thật thành accepted drift chỉ để làm xanh. Tách PR theo nhóm repro, không tăng số issue nếu cùng phạm vi.",
  },
  {
    title: "#345 và PR #347 — không nhập vào redesign",
    url: "https://github.com/Nuncio-hq/crew/issues/345",
    action:
      "#345 là thảo luận độ sâu fork delta. #347 là PR docs đang mở; Source snapshot came from its HEAD with dirty CompanyOS source; Stage 0 excludes its committed topology hunks. Review/reconcile docs trước khi tạo PR implementation từ main.",
  },
  {
    title: "Closed issues là lịch sử, không phải backlog mới",
    action:
      "Tái dùng #116 roles, #190 plans, #193 PR hub, #196/#197 tools, #169 sessions, #173 handover, #187 workspace, #200 Wiki và các agent lifecycle issues. Không reopen hàng loạt. #102/#151 đóng vẫn không chứng minh Mission orchestration đã được implement.",
  },
];

export const managementContract = {
  deletion:
    "Delete agent có preview ảnh hưởng và xác nhận trong app. Prototype tombstone ID để giữ tên tác giả, bỏ directory/DM/pickers, lọc role holders và contact; role definitions giữ Unassigned. Runtime/profile/worktree và hội thoại không bị xóa. Production dùng delete_managed_agent và orchestration xóa hiện có: stop process, assignment cleanup/recovery, key removal, tombstone và archive identity. Deployed remote agents có guard riêng, không auto force. Canvas contact point mới phải có cleanup/retry và đồng bộ nhiều client; không coi thao tác UI local là chứng minh cascade hoàn tất.",
  recap:
    "Thread context hiện là các trường dữ liệu, không do model tổng hợp. Recap là phần tổng hợp dễ đọc riêng, mặc định Off; Settings chọn runtime từ discovery (prototype chỉ có inventory mẫu), Hermes chọn profile, Codex/Claude chọn model hoặc default. Generate/Regenerate chỉ theo yêu cầu. Không thay đổi execution model của agent, không dùng recap làm acceptance. GlobalAgentConfig và runtime discovery là seams; handover_summarizer_model/BUZZ_ACP_HANDOVER_MODEL hiện phục vụ guided handover, không tự cung cấp một recap đa-runtime cho user. Backend recap mới cần kiểm tra runtime/model/auth, nguồn thread được phép đọc, giới hạn context/chi phí, cancel/timeout, lỗi retry và lưu nguồn/revision/time để nhận biết stale. Prototype nối một đoạn text mẫu và lưu tên cấu hình; không gọi CLI/model thật. Stale demo so sánh trường thread, số tin nhắn và identities; production cần watermark sự kiện thật.",
  plans:
    "ACP acp.rs nhận sessionUpdate:plan. declaredPlanSnapshot.ts đã parse ACP plan và structured todo tools; declaredPlanProjection.ts giữ snapshot mới nhất theo agentPubkey + conversationId, loại retired sessions và empty plan clears. Reuse model đó cho tab Agent plans: mỗi agent riêng, trạng thái pending/in_progress/completed từ runtime, source/session/seq/time, không LLM. Không gộp plan của nhiều agents thành một authoritative task list; missing/disconnected/retired phải rõ. Prototype có snapshot mẫu theo timer cho Hermes/Codex và todo sample Claude, unknown ở thread khác; chưa xác minh adapter live và quyền owner của observer. Unit tests runtime matrix chỉ chứng minh parser/projection, không chứng minh mọi CLI đang emit plan.",
  contactPoint:
    "Channel contact point là lựa chọn một member theo ID/pubkey, độc lập role và thread owner. None giữ luồng mention-only. Human message không mention trong channel hoặc thread gửi tới contact; explicit mentions không gọi thêm contact; agent-originated messages không kích hoạt fallback. Prototype match tên trong text chỉ để demo, production cần mention targets có cấu trúc và author identity đã xác minh. Buzz SubscriptionRule.require_mention và relay #h/#p filters là seam nhận tin sẵn có; respond_to là author authorization, không phải wake policy. Cần mở rộng owner-signed canvas (crew_role.rs) bằng contact pubkey, cập nhật subscription và dispatch có dedup/generation fence khi contact thay đổi; giữ access, kinds, author gate, budget và queue. Routing presets hiện có không tự đánh thức contact. Chưa code backend; cần test relay thực về mention priority, bot-loop, reconnect, hai client, membership/revocation và đổi contact giữa các tin.",
  agents:
    "CreateManagedAgentRequest / UpdateManagedAgentRequest có name, model và hermes_profile; list_hermes_profiles cung cấp profile discovery. Prototype dùng ID ổn định riêng, production phải dùng pubkey. Add agent và join channels không phải một transaction được chứng minh: cần xác minh từng acknowledgement và giữ retry/recovery khi partial failure. Model ID nhập tay không chứng minh model tồn tại hoặc có quyền dùng. Hermes chỉ chọn profile, không ghi đè model/provider của profile.",
  roles:
    "Canvas crew block đã có definitions độc lập với assignments, nên role Unassigned không đòi thêm registry. Add role lưu định nghĩa; checkbox chuyển holder giữa role trong một snapshot. Không thêm quyền tool hay đổi chủ thread. Prototype chỉ cho chọn thành viên channel từ directory mẫu.",
  checkout:
    "Worktree registry/detail là nguồn branch và checkout thực; thread_github.rs cung cấp head/base, additions/deletions, isDraft và state. thread_forge/diff.rs phân biệt worktree diff với API diff. Card ghi local vs HEAD; PRs ghi PR diff tại revision cụ thể. Không cộng hai loại số, không tạo worktree cho non-code, và không coi checkout đã dọn là còn chạy.",
};

feasibility.push({
  title: "Wiki v0.9 — publication, runtime, retrieval and private handoff",
  status: "Demonstrated flow accepted; production proofs/wiring remain",
  source:
    "crew-wiki publish.rs/generate.rs; useWikiGenerate/useWikiRefresh/useWikiEventsQuery; wikiEvents; WikiPageView/WikiSourceFiles; current WikiAskBox and wikiAsk.ts use fixed examples.",
  remaining:
    "#362 coherent snapshot/retention and shared G-DURABLE; #363 installed-runtime immutable-source generation; #364 full-body scoped NIP-50 and immutable local blob reads; #365 private existing-agent proof; #366 real Ask/history; #367 durable explicit dispatch and private-safe backlink.",
  limit:
    "TOC-last alone cannot preserve replaced old pages. No generic Wiki/project outbox exists. Local file reader uses current bytes; copied source excerpts prove only this reference. Mock answer/history/timers are not runtime, privacy, persistence or receipt evidence. Company Wiki menu is coordinator-proposed compatibility, not an accepted new store.",
});

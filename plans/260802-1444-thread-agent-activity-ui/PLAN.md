# Agent activity trong thread — implementation plan

Prototype: [`prototype.html`](./prototype.html) (bấm mở trực tiếp, không cần server).

## Outcome

Thay phần hiển thị hoạt động agent trong channel + thread bằng mô hình 3 bề mặt:
thread hẹp/channel = 1 dòng, thread mở rộng = thẻ công việc của thread (nhiều
agent), xong việc = 1 dòng kết quả ở lại vĩnh viễn. Chi tiết kỹ thuật nằm sau
1 cú bấm, mở cạnh chứ không nuốt hội thoại.

## Non-goals

- Không thêm i18n framework trong scope này (xem "Ngôn ngữ" bên dưới).
- Không dịch nội dung do agent viết ra.
- Không đổi giao thức observer, không thêm event kind mới.
- Không đụng mobile trong 6 phase đầu.

## Ngôn ngữ: chrome vs content

Hai lớp chữ, hai chủ sở hữu khác nhau:

| Lớp | Ví dụ | Ngôn ngữ | Ai quyết |
|-----|-------|----------|----------|
| **Chrome** — chữ app tự viết | "is working", "Stop", "3 agents", "Step 3 of 5", "Done in 2m14s" | Theo app | Mình |
| **Content** — chữ agent viết | từng dòng checklist, câu hỏi agent hỏi | Theo agent | Agent |

Hiện trạng đã kiểm: desktop **không có i18n**. `desktop/package.json` không có
i18next / react-intl / lingui, và ngày giờ đang pin cứng `en-US`
(`desktop/src/features/messages/lib/dateFormatters.ts:12`). Nên chrome sẽ là
**tiếng Anh**. Tiếng Việt trong prototype chỉ để đọc cho nhanh.

Checklist thì hiện **nguyên văn** những gì agent viết — mảng `{text, done}` từ
tool todo, không dịch, không sửa. Agent viết tiếng Việt thì checklist tiếng
Việt trong khi khung vẫn tiếng Anh. Trộn như vậy là **đúng**, không phải lỗi:
dịch lại lời agent sẽ làm sai lệch việc nó đang làm.

Hệ quả cho code: chrome phải là **tập chữ đóng, gom một chỗ** (~20 chuỗi trong
một module hằng số), để sau này thêm i18n là sửa 1 file. Muốn checklist ra
tiếng Việt thì đòn bẩy là **system prompt của agent**, không phải app.

## Cái đã có (đã đọc, không phải xây lại)

| Thứ | Ở đâu |
|-----|-------|
| Id riêng cho từng thread | `conversationId.ts` — `deriveAgentConversationId(channelId, rootEventId)` |
| Agent nào đang chạy trong 1 thread | `activeAgentTurnsStore.ts:512` — `getActiveAgentsForConversation()` |
| Gom cấp channel + `agentCount` | `activeAgentTurnsStore.ts:535` — `getActiveTurnsByChannel()` |
| Tín hiệu "đang làm việc" hợp nhất | `agentWorkingSignal.ts` (observer chính, typing dự phòng) |
| split / focus | `threadViewModePreference.ts`, nút `ThreadViewModeToggle.tsx` |
| Cột 880px + sliver 72px | `threadFocusLayout.ts` |
| Huỷ turn | `shared/api/agentControl` — `cancelManagedAgentTurn` |
| Transcript đầy đủ | `AgentSessionThreadPanel.tsx` |
| Hợp đồng tool todo | `crates/buzz-dev-mcp/src/lib.rs:87` — mảng `{text, done}` |

## Progress

- [x] Phase 1 — todo array + `agentPlanProgress`
- [x] Phase 2 — `getActiveTurnsByConversation()`
- [x] Phase 3 — static activity line (no headline rotation)
- [x] Phase 4b — sticky project-thread status bar (Oscar-approved)
- [ ] Phase 4 — ThreadWorkCard checklist (extends 4b in focus)
- [ ] Phase 5 — multi-thread rollups
- [ ] Phase 6 — done footer + detail rail

## Phase 1 — Giữ lại mảng todo, suy ra bước hiện tại

**Vấn đề:** `agentSessionToolClassifier.ts:629` (`getTodoPreview`) nhận đủ mảng
`args.todos` rồi **bóp thành đúng 1 chuỗi preview và vứt phần còn lại**. Không
có mảng thì không có checklist.

- `agentSessionToolClassifier.ts` — trả thêm `todos: {text, done}[]` bên cạnh
  `preview` hiện có. Không đổi `preview` (còn chỗ khác dùng).
- `agentSessionTypes.ts` — plan item mang thêm `todos`.
- Module mới `agentPlanProgress.ts` — từ transcript của (agent, conversation)
  suy ra `{ steps, currentIndex, updatedAt }`. Bản todo mới nhất thắng.
  `currentIndex` = phần tử `done: false` đầu tiên.
- **Degrade:** shape lạ (harness khác) → trả `null`, mọi bề mặt rơi về 1 dòng.

**Test:** classifier giữ nguyên mảng · bản cập nhật sau đè bản trước · mảng
rỗng / thiếu field → `null` · shape lạ → `null` chứ không ném lỗi.

## Phase 2 — Gom theo thread

- `activeAgentTurnsStore.ts` — thêm `getActiveTurnsByConversation()` +
  `useActiveTurnsByConversation()`, cùng shape `ActiveChannelTurnSummary`
  nhưng khoá theo `conversationId`. Dùng đúng pattern cache + invalidate của
  `getActiveTurnsByChannel()` đang có.
- Reset theo community: nhớ thêm vào `resetActiveAgentTurnsStore()` nếu có cache mới.

**Test:** soi theo `activeAgentsForConversation.test.mjs` đang có — nhiều agent
1 thread, 1 agent nhiều thread, cache invalidate khi turn kết thúc.

## Phase 3 — Bỏ headline xoay vòng, thay bằng 1 dòng đứng yên

Đây là phase **ship được độc lập** và đã đủ giải quyết than phiền gốc.

- `BotActivityBar.tsx` — xoá `HEADLINE_ROTATION_MS` (dòng 31) và toàn bộ
  `activityHeadlines` + `headlineIndex`. Thay bằng: pulse + `<tên> is working`
  (1 agent) hoặc `N agents working` + đồng hồ + Stop.
- Ngưỡng im lặng: turn < 3s → không hiện gì.
- Phát hiện kẹt: > 90s không có frame mới → chuyển amber + Stop tại chỗ.
- 2 chỗ gọi giữ nguyên API: `ChannelComposerActivityAccessory.tsx`,
  `useThreadComposerBotActivity.ts`.

## Phase 4b — Sticky project-thread status bar (Oscar-approved)

Động lực: `ProjectThreadWorkspacePanel` nằm trong vùng cuộn → phải kéo lên
mới thấy Task/Workspace/Handoff. Dữ liệu agent đã có sẵn trong panel.

**Hai luật đã chốt:**
1. `split` = chỉ 1 dòng thu gọn; lưới 3 ô + GitHub chỉ ở `focus` (expand).
2. Project thread → thanh sticky sở hữu tín hiệu agent; tắt dòng composer
   bot activity cho thread đó (typing vẫn hiện). Thread thường giữ composer.

**Làm:** dời panel ra ngoài vùng cuộn (giữa header và message list), thu gọn
thành 1 dòng (agent · bước · elapsed · chip chấm · Stop · expand ở focus).
Không đụng drawer. Phase 4 checklist sẽ mở rộng thanh này, không thêm thanh thứ hai.

## Phase 4 — Thẻ công việc của thread (chỉ ở chế độ mở rộng)

Component mới `features/agents/ui/ThreadWorkCard.tsx` — **mở rộng phase 4b**,
không mount thêm dải trạng thái thứ hai.

- Chỉ mount khi `useThreadViewMode() === "focus"` và thread có agent đang chạy.
- **Thẻ thuộc về thread, không thuộc về agent.** 1 agent → checklist mở sẵn.
  Nhiều agent → mỗi agent 1 dòng, bấm mới mở, **chỉ 1 dòng mở cùng lúc**.
- Thứ tự: đang hỏi người → đang chạy (lâu nhất trước) → đã xong.
- Trần 4 dòng + "+N nữa". Thu gọn cả thẻ thành 1 dòng, nhớ lựa chọn
  (theo pattern `transcriptAnimationPreference.ts`).
- Stop từng agent + Stop tất cả.
- **`getAgentPlanProgress` phải lọc theo `conversationId`.** Phases 1–3 chỉ
  lọc `channelId` + `turnId`. Một agent chạy hai thread trong cùng channel sẽ
  lẫn checklist nếu thẻ phase 4 nhìn theo thread. Thêm `conversationId` vào
  `options` và so với `TranscriptItem.conversationId` (đã có sẵn). `turnId`
  vẫn hữu ích khi caller biết turn, nhưng thẻ thread không được phụ thuộc vào đó.

**Quyết định layout — thẻ nằm ngoài vùng cuộn.** Mount giữa header panel và
danh sách tin nhắn, **không** đặt `position: sticky` bên trong vùng cuộn.
Lý do: `useAnchoredScroll.ts` gắn `ResizeObserver` lên cả content lẫn container
(dòng 791–804) để neo scroll; một phần tử **đổi chiều cao** (mở/đóng accordion)
nằm trong vùng cuộn sẽ đánh nhau với cơ chế neo đó và làm nhảy vị trí đọc.
Prototype đặt sticky trong vùng cuộn — chỗ này bản thật phải làm khác.

## Phase 5 — Nhiều thread

- `MessageThreadSummaryRow.tsx` (~dòng 253–274) — khi conversation đó có agent
  chạy, thay `last reply X ago` bằng `● N agents working · 2:46`. Không thêm UI
  mới, chỉ đổi nội dung dòng đã có.
- `BotActivityBar` cấp channel — gom `N agents in M threads ▸`, bấm ra popover
  danh sách thread, chọn để mở thread đó.
- Header drawer — chip `M other threads running`.
- Badge sidebar — đổi từ đồng hồ sang **số agent** (nhiều thread thì đồng hồ
  của thread nào cũng sai; con số luôn đúng).

## Phase 6 — Dòng kết quả + rail chi tiết

- Dòng kết quả dưới message cuối của agent: `✓ Done in 2m14s · 3 agents ·
  12 steps · View details`. Muted, 1 dòng, ở lại vĩnh viễn.
- `AgentSessionThreadPanel` — mở thành **rail bên phải trong drawer** thay vì
  thay thế pane thread. Ở focus mode cột chữ chỉ 880px
  (`threadFocusLayout.ts`) nên còn dư chỗ.
- Rail thuộc về **1 agent** — mở từ dòng agent nào thì hiện việc agent đó.

## Rủi ro

| Rủi ro | Xử lý |
|--------|-------|
| Harness khác không có tool todo | Mọi bề mặt phải chạy được khi `agentPlanProgress` trả `null` → rơi về 1 dòng. Test riêng cho nhánh này. |
| Thẻ đổi chiều cao làm nhảy scroll | Đặt ngoài vùng cuộn (Phase 4). |
| Dòng checklist quá dài / khác hệ chữ | Cắt 1 dòng + ellipsis, không xuống dòng. |
| Chrome tiếng Anh lẫn content tiếng Việt | Là hành vi đúng, ghi vào docs để không ai "sửa" nhầm. |

## Thứ tự đề xuất

`1 → 2 → 3` rồi **dừng lại xem**. Phase 3 đã bỏ được cái xoay vòng và là phần
gây khó chịu nhất hiện nay. Phase 4 là phần chính nhưng cũng tốn nhất — chỉ
làm sau khi phase 3 chạy thật vài ngày.

## Câu hỏi còn mở

1. Chrome tiếng Anh — có muốn thêm i18n luôn không, hay gom string một chỗ rồi để sau?
2. Muốn checklist ra tiếng Việt thì phải set trong system prompt của agent. Có đặt mặc định đó cho crew không?
3. Thẻ dính trên đầu hay trôi theo hội thoại? (đề xuất: trên đầu, ngoài vùng cuộn)
4. Mobile — phase mấy, hay để sau hẳn?

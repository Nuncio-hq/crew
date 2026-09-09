# CompanyOS blueprint v0.1 — visual and interaction QA

final result: passed

Scope: founder-selected layout adaptation, not a pixel clone of Codex or
verification of Crew's real backend. All detailed flows remain proposals.

## Evidence and normalization

- Source visual: `public/references/codex-layout.png`, 5120 × 2826 pixels.
- Installed-app baseline: `public/references/crew-focus-before.png`.
- Accepted rendered capture: `.qa/workspace.png`, 1119 × 1089 pixels.
- Browser: Codex In-app Browser, normal CSS viewport approximately 1120 × 1089;
  reported devicePixelRatio 2.4, with the CUA screenshot normalized to CSS scale.
- State: `thread-working`, Files open, Blueprint notes closed.
- Full comparison: `.qa/comparison.png`, source and implementation together,
  normalized to 700 px wide with aspect ratio retained.
- Focused comparison: `.qa/sidebar-comparison.png`, both actual sidebar crops
  scaled to 260 px wide. All three grouping levels, selected row and footer read
  correctly. Local QA captures are ignored by Git; supplied references are retained.
- Viewports differ: the reference is wide, the user's current browser is tall.
  Compared hierarchy, relative pane ownership, palette, typography and controls;
  no claim of pixel equivalence. Review toolbar, sample content and responsive
  proportions are intentional adaptations to the founder's request.
- A temporary explicit viewport produced a scaled screenshot with blank margins;
  rejected that capture (`workspace-1440.png`) and restored normal browser sizing.
  The accepted capture above has no blank capture margins or horizontal overflow.

## Findings and fixes

No outstanding P0/P1/P2 findings within this prototype scope.

1. P2: Tools labels disappeared from accessibility names at the compact breakpoint.
   Added explicit accessible names and titles independent of visible tab text.
   Verified Files, Browser, PR, Terminal and Context by accessible locators.
2. P2: Review notes could compete with both app panes at intermediate widths.
   Suppressed the tool pane while the external notes rail is open at those widths.
   Visually verified notes plus a readable conversation in the normal viewport.
3. P2: Scenario changes could preserve the Activity tab and conceal a pending
   decision. Scenario identity now resets that tab; needs-you/done scenarios
   scroll their current action/report into view. Verified the decision and receipt.
4. Interaction defects found during inspection: icon buttons inside the composer
   now explicitly use type=button; project navigation carries the selected project
   instead of showing another project's name. Removed out-of-scope secondary links
   that incorrectly opened the primary thread.

## Required visual surfaces

- Typography: locally bundled Inter, restrained weights, 12–14 px conversation
  copy at desktop sizes; headings and navigation are distinct. Long thread title
  truncates in the sidebar without changing the full central title. Vietnamese
  review notes use the bundled Vietnamese font subset.
- Spacing/layout: left workspace/project/DM groups, central conversation and
  adjacent tools follow the reference. Composer stays anchored. No permanent plan
  column. Normal conversation width is roughly 480 px at the user's current viewport.
- Colors/tokens: dark neutral panels and subtle selection match the source direction;
  muted green/amber state accents and violet review chrome are purposeful proposals.
- Assets: source references retained, licensed Inter and Lucide icons bundled.
  No raster content is required in the app-owned sample screens. Identity glyphs
  are UI icons, not claims to reproduce provider brand logos.
- Copy: sample work uses Crew project/channel/agent vocabulary. No raw Nostr IDs
  or auth/config details in the app flow. Review explanations stay outside the app.

## Browser interaction checks

- Channel → replies → central thread → Back to channel.
- Conversation → Activity; activity UI renders without replacing thread identity.
- Needs you: choice initially disabled; selecting a radio enables confirmation;
  confirming appends a local reply and returns to Working.
- Disconnected: send disabled, draft retained, Reconnect restores send state.
- Direct message: correct recipient, local send visible; directory is separate.
- Files / Browser / PR / Terminal / Context tabs display the relevant sample content.
- Inbox, Wiki and Workflows render; workflow Pause changes to Resume locally.
- Keyboard divider: ArrowLeft changes the split from 46 to 48 percent.
- Narrow CSS viewport 800 × 750: no horizontal overflow; tools occupy the available
  main area, Close tools restores the readable conversation and composer.
- Completed thread: report visible, zero Stop buttons.
- Blueprint notes rail visibly outside the app; accepted/proposed labels present.
- Browser error/warning log check returned no entries after these interactions.
- `npm run build` passed. No native app, real relay, live agent, auth or CI-latency
  validation is claimed. This is not a repository-wide CI run or release gate.

## Remaining review decisions

Founder review: project hierarchy, default right tools, final widths, historical
activity access, and control placement. Full keyboard/screen-reader audit and
native streaming/agent behavior require production implementation and real-flow
validation. The prototype's simpler in-memory state must not become production
authority. Do not interpret this QA pass as founder acceptance of all proposals.

## Document/UI split — 2026-09-08

- `index.html` now renders the Vietnamese design document; `prototype.html`
  preserves the UI. Both entries are emitted by the Vite build.
- Inspected the document overview and Projects section in Codex Browser.
  Sidebar table of contents, readable explanation rows and boundary callouts
  have no visible clipping at the normal browser size.
- Verified Projects anchor resolves to the section; its prototype link opens
  the channel state; opening replies retains the prototype.html URL; Design
  document returns to index.html. Source scenario rules remain shared.
- Existing UI design comparison above applies to the preserved UI, now at
  prototype.html. Document presentation follows the founder-requested separation
  and HeardBack-style explanatory reference, not the Codex UI visual target.
- Final build passed with both HTML files; git diff --check passed.

## v0.2 — channel triage, scoped tools and delivery evidence (2026-09-09)

Source: founder's three annotated v0.1 channel/PR screenshots in this task;
layout baseline `.qa/workspace.png` (1119×1089). Implementation:
`.qa/channel-v02.png`, `.qa/thread-v02.png`, `.qa/actions-v02.png` and
`.qa/lifecycle-document-v02.png`. Normal desktop capture is 1120×1089 pixels;
comparison normalizes both thread-working / Files views to 700 pixels wide in
`.qa/v01-v02-comparison.png`. This is an intentional content/behavior iteration,
not a pixel clone of the defective channel scope. Combined comparison opened;
full-size channel and Actions captures separately inspected for readable labels.
No additional focused crop needed: full-size captures expose all relevant labels.

Findings addressed: thread tools previously remained accessible at channel scope;
channel lacked attention/completion summaries; PR evidence assumed a single PR.
Tools now render only inside a selected thread and show its title. Seven thread
cards expose owner/status/next action and PR summaries; PR and Actions selectors
show revision-specific checks, review and delivery separately. A stopped turn and
accepted work have distinct states. A stale production-decision label was changed
to the factual fixture state, production not started.

Typography: bundled Inter and existing heading hierarchy retained. Spacing:
sidebar and conversation/tool proportions match the baseline; extra scope row is
intentional. Colors: existing dark tokens retained with text labels for semantic
states. Assets: existing Lucide icons, no new raster assets. Copy: shorter
thread-specific conversations and explicit acceptance criteria replace the single
workspace script. The document labels lifecycle and provider rules as proposals.

Browser checks passed: channel filters; Accept -> Inbox count 2 to 1 and Done
count 1 to 2; accepted work Reopen -> In progress; two-PR selection; Actions logs
for running and failed jobs; no other thread's PR shown; draft isolated and
restored; decision disabled until selected; completed scenario; reset; channel
has seven cards and zero thread tool panels. At measured CSS 800×750, no horizontal
overflow and closing tools exposes the composer. Document lifecycle anchor and
return-to-channel link exercised. Browser console contained three earlier Vite
reload errors during the temporary syntax edit at 14:35:46 UTC; builds and
subsequent interactions after the fix showed no new runtime errors.

Validation: two-entry production build and git diff --check passed. Provider
execution, multi-task aggregation, authoritative acceptance storage and runtime
integration remain unimplemented. No live relay, GitHub writes or deployments.

final result: passed

## v0.3 — visible cards, roles and purposeful motion (2026-09-09)

Source: founder's annotated v0.2 card screenshot, represented by local
`.qa/channel-v02.png`. Final implementation `.qa/channel-v03-final.png`.
Both captures are 1119×1089; combined `.qa/v02-v03-comparison.png` normalizes
both to 700 pixels wide. The combined comparison was opened and inspected.
Full-size `.qa/roles-v03.png` and `.qa/demo-drafting-v03.png` cover responsibilities
and the opt-in demo. Final normal CSS viewport approximately 1120×1089; compact
viewport measured 800×750. Card content differs intentionally per the request.

Fable 5.1 consulted through Claude Code CLI with medium effort after the user
restored login. It performed a read-only review. Its five findings informed the
second pass: consistent attention accent, visible demo target and review CTA,
lightly staggered entrances and single keyboard outline, aligned overview/filter
counts, dedicated running/error icons. All implementation remained local here.

Typography: existing Inter preserved; card title, muted context, next action and
footer have a deliberate hierarchy. Layout: separate padded surfaces replace
bare dividers, trading a little density for clearer grouping; expanded roles are
opt-in. Tokens: amber edge means attention, red means execution failure, green/
blue check chips retain textual labels. Imagery: existing Lucide icon system
with dedicated LoaderCircle and failure glyphs; no new raster art. Copy: roles
are work-specific responsibilities, not permissions or online status.

Browser checks: role expansion shows named Lead/Implementation/Review scopes;
#402 badge opens #402 selected; demo begins Drafting/Working and terminates at
Ready for review; Review result opens the matching thread; explicit Accept
records Done and ends demo; Reset returns original samples. Compact layout has
no horizontal overflow. Final browser error log is empty. The reduced-motion
CSS override disables animations, transitions and smooth scrolling; OS-level
reduced-motion emulation was not exercised. Timer is bounded to two advances;
state navigation/reset cancels it and End demo restores the sample checklist.

Earlier v0.3 capture `.qa/channel-v03.png` used different edge colors for decision
and review attention and hid role names at this viewport. Final capture keeps
both attention edges amber and displays role names at desktop widths. No
remaining P0/P1/P2 findings. Static screenshots cannot demonstrate timing; use
Play activity demo in the running prototype to review motion.

Validation: both HTML entry builds pass. No relay or GitHub execution. Role
assignment storage, job telemetry and real check/deployment updates remain
proposals outside this prototype.

final result: passed

## v0.4 — autonomous workspace film and brand SVGs (2026-09-09)

Source visual baseline: `.qa/channel-v03-final.png`. Final matching channel view:
`.qa/channel-v04.png`; both 1119×1089 pixels. The opened combined comparison is
`.qa/v03-v04-comparison.png`, each side normalized to 700 pixels wide. The new
film is an intentionally different interaction mode, verified in full-size
`.qa/film-copy-v04.png` and `.qa/film-running-v04.png` at the normal ~1120×1089
CSS viewport. Avatars are readable in these full-size captures; no extra crop
was necessary. Marketing copy and schedule are intentional new content.

The 132-second movie autoplays without app clicks. It types a founder brief,
creates the launch thread, streams Hermes/Claude/Codex messages, moves the story
cursor, opens campaign copy, crosses to engineering, shows CI failure/recovery,
and returns for a scripted marketing handoff. Playback chrome stays explicitly
simulated. The timeline is a pure absolute-time projection, not accumulated
state mutation. Fable consulted on the storyboard at medium effort; local work
implemented and verified it.

Typography/layout: same Inter, card hierarchy and two-pane shell; the film
transport adds deliberate top chrome. Color: existing attention and check
semantics preserved. Assets: generic agent symbols replaced with unchanged-path
Lobe Icons SVGs for Claude, Codex and Hermes Agent; provenance and MIT license
saved in public/brands. Standard actions remain Lucide SVGs. Copy: marketing,
audience, customer follow-up and coding dependency use distinct scopes. No
performance metrics are invented as observed live results.

Browser verification: autoplay visibly typed the brief and streamed replies;
full 4x playback reached 132 seconds, stopped automatically and showed the
campaign Done. Keyboard End/Home on the timeline removes future threads/messages
when rewound. Chapter seeking, pause (time held at 0.5 during inspection), speed,
restart, marketing artifact and Explore app were exercised. Exploration retained
the campaign document; #customers opened the pilot follow-up. Compact measured
800×750 viewport has no horizontal overflow and accessible playback controls.
The earlier pointer label could clip at the right edge; its bounds and target
recalculation were fixed and the copy-scene screenshot confirms it stays inside.

Projection checks exercised 529 timestamps for valid thread/channel scopes,
plus explicit assertions for typing, thread arrival, failed checks, corrected
revision, acceptance, reverse seeking and immutable seed fixtures. Two-entry
build and git diff --check passed. Console history contained earlier development
HMR/module-name errors from 15:19 UTC, fixed by renaming FilmPlayer.jsx to avoid
case-insensitive resolution against film.js; no new runtime errors appeared in
the completed playback. Reduced-motion CSS remains in place; OS emulation was
not performed. A movie acceptance is fictional and has no external authority.

final result: passed

## v0.5 — backend-grounded activity and channel roles (2026-09-09)

- Read the current role canvas parser/resolver, desktop canvas command, ACP observer emission, encrypted ephemeral observer contract, transcript reducer and conversation-scoped headline selector. These establish an implementation path, not an end-to-end live integration.
- Ran the existing desktop `agentSessionTranscript.test.mjs` and `observerCoalescedNotify.test.mjs`: 59 passed. They exercise message coalescing, canonical tool updates, identity/context retention and notification batching. No production code changed.
- Prototype assertions passed for role inheritance independent of thread owner, missing roles and unknown participants, incremental text, tool-row replacement, all 529 sampled film frames and reverse seek before a failure.
- Browser verified the highlighted summary is replaced on working cards; sample text starts partial then advances to received actions. Channel roster and thread participants show the same inherited roles. View activity opens the correct thread with all four events from the compact card's source. Disconnected card retains last-received data with current progress unknown.
- At the available 635px viewport the app has no horizontal overflow. Visually checked the card and the new feasibility document. Fixed initial hash navigation so `/index.html#feasibility` reaches the section after React mounts; verified section top at 42px. Sidebar document navigation is hidden at this width, so the intro also links to feasibility.
- Dual-entry build and four Sites packaging tests passed; repository `git diff --check` passed. Earlier development-server/HMR errors were followed by a successful build and fresh-page checks; no live relay or engine session was run. Runtime-specific thought availability, owner permissions, reconnect recovery, and all external integrations still need real verification before shipping.

### Channel role assignment editor (v0.5 follow-up)

- Visually checked the owner-view modal: channel identity, member-specific role/definition fields, remove, Cancel and Save. Native dialog supplies modal focus handling and Escape dismissal.
- Browser verified changing Codex to Developer in #product updates the channel card and opened thread. #engineering still shows Implementation. Removing Claude's assignment changes thread participants to No channel role while preserving participation.
- Whitespace-only responsibility blocks Save with an inline alert. Cancel restores the saved definition; Escape discards an unsaved Draft only label. #general exposes Assign roles with unassigned member fields.
- Save affects one in-memory channel snapshot. Reset/reload restores samples; no relay writes, membership changes or tool grants. The design document records the canvas integration and unresolved concurrent-edit guard.
- Build passed. Runtime canvas persistence/auth/conflict handling remains unimplemented in this prototype.

## v0.6 — Add flows, runtime configuration and Git metadata

- Browser exercised Add role → QA reviewer definition → Save unassigned → Add agent (Hermes, research profile, #product) → assign that new agent to QA reviewer. Roster displayed four roles/four agents; definitions and holders survive navigation.
- Verified Hermes form has no Model ID field. Edited Codex's Model ID to a sample value and confirmed it on the directory card. Reset removed the test agent, new role and sample model override.
- Visually checked the directory and Hermes add dialog at 1130×1089, plus the branch/worktree card. Card shows local +128/−42 vs HEAD; selecting its worktree opens the scoped path in Context. PR #401 separately shows +284/−91 against main at layout-a3.
- Browser readback of actual PR text colors: Open rgb(63,185,80); Draft rgb(161,168,179); Merged rgb(188,140,255); Closed rgb(248,81,73). CI is independent. The accepted thread retains a superseded Closed PR alongside the Merged PR.
- Build and four Sites packaging tests passed; git diff check passed. Assertions covered all 529 film frames, valid PR references, non-code work having no checkout, and no tracking local diff before the story reaches editing.
- All additions remain simulations. No models were invoked, agents started, profiles changed, members added to a live relay, or GitHub data written. Source/permission/recovery requirements are in the maintained design document and architecture boundary.

## v0.7 — Channel contact point

Added optional member selection in Assign roles, saved atomically with role definitions and holders in the local sample state. The channel and composer show the contact. Browser verification: choose Hermes and Save; an unmentioned channel message receives one Hermes sample reply; an explicit @Codex receives only Codex; the same fallback works in a thread. Canceling a draft change to Claude preserves Hermes. Switching to engineering shows mention-only while returning to product preserves Hermes. The editor screenshot was reviewed at 1130×1089 with readable selector/help, scrollable fields and visible Save/Cancel footer.

Validation: Vite build and all four Sites worker checks pass; ten assertions against the actual prototype routing helper cover fallback, explicit/multiple/unknown mentions, None, channel isolation, agent-originated messages, unavailable/removed contact and stable identity after rename. `git diff --check` passes. Production relay subscriptions and dispatch remain unimplemented; no live-agent behavior is claimed. Existing architecture/design document describe the required canvas extension, author gate, subscription updates and live tests.

## v0.8 — Delete agent, recap settings and agent plans

Browser checks at 1130×1089: Delete Codex opens an effect preview; Cancel retains all three agents. Confirm removes Codex from directory and DM navigation, clears its product contact, removes holder/picker entries and preserves Implementation as Unassigned. Historical thread identity remains. Settings saves Hermes/research with no model override and supports an optional Codex model. Generate displays the saved configuration label; no runtime is invoked. Default Off disables Generate. A newer thread message marks an earlier sample recap Out of date. Recap runtimes remain independent of agent instances.

Agent plans shows separate Hermes/Codex ACP snapshots and Claude structured-todo examples with Pending, In progress and Completed. The disconnected scenario keeps its last-known plan and states that current execution is unconfirmed. Settings and plan screenshots were inspected for readable controls and scrolling. Existing backend snapshot/projection/runtime-matrix tests: 27 passed. Vite build and four Sites checks passed. No live deletion, runtime discovery, recap generation or ACP adapter run was performed.

## Stage 0 public source handoff — 2026-09-09 (#344)

Documentation RED: clean main at `8278d2b14cd04e17c1010d819c9fdd20eaefff9a`
has no `design/companyos`; FORK/D-066 prohibit Projects, D-056 retains permanent
plan placement, and PRODUCT describes a Workbench door despite D-065 redirects.
The accepted conceptual shell split is recorded; detailed UI and G-THREAD-1
remain proposed. Production guard changes belong to #349. No artificial unit
test or mutation test was added for this source/docs-only change.

GREEN in the isolated worktree: `npm ci`, `npm run build`, and
`npm run test:sites` (4 passed, 0 failed/skipped). Both HTML entries and
Sites-compatible output exist. Browser at task-owned port 4344 resolved all 15
named state IDs with correct selected scenario and simulation label: settings,
agents, channel, thread-working, needs-you, offline, dm, empty, inbox, wiki,
workflows, done, completed, blocked, discussion. No broken images were observed.
Document hash links and local Markdown file targets resolve. Original private
captures are excluded; document and Blueprint panel links use safe source/state
references. Narrow 800×700 check: no document horizontal overflow, Settings
opens with Enter, scenario selection reaches Agents, Blueprint opens with Enter,
and Close tools dismisses the pane with Enter. Browser console had no errors.
These checks establish reference usability only, not production behavior.

The 58-file founder source/doc fingerprint matched before/after copying and
again after validation. No founder file, daily data, agent or runtime was changed.
PR #347 committed topology hunks and unrelated dirty HERMES/STATE/templates
are excluded. Final repository `just ci`, remote Gate, independent exact-head
reviews and post-merge downstream pinning remain pending coordinator scheduling.

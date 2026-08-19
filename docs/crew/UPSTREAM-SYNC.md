# Upstream Sync Runbook

## Repository topology

```text
upstream  https://github.com/block/buzz.git      fetch only
origin    https://github.com/Nuncio-hq/crew.git  fetch and push
```

Crew is a GitHub fork, not an unrelated copy. Keep GitHub's fork relationship
and the local `upstream` remote.

The machine-readable baseline is
[`upstream-buzz.json`](upstream-buzz.json). Updating it is an explicit sync
decision, not a side effect of building or releasing NuncioCrew.

The local upstream push URL is deliberately disabled. Never push to
`block/buzz`.

## Thin-fork rules

- Prefer Crew-owned files under a new namespace.
- Do not restyle or reorganize upstream code.
- Do not copy upstream modules into parallel Crew versions.
- Existing upstream-file edits require explicit justification.
- The normal UI integration budget is one route-registration edit and one
  navigation-entry edit.
- A Rust edit requires a failed or insufficient non-Rust spike plus explicit
  approval.
- Keep `main` green.
- Use short-lived area branches; do not create long-lived component branches.
- Perform upstream integration on `sync/upstream-YYYY-MM-DD`.

## Upstream files Crew edits

| File | Justification | Resolve hint |
| --- | --- | --- |
| `crates/buzz-acp/src/base_prompt.md` | office-level behavioral rule belongs in the office-level prompt | self-contained Markdown section — on conflict, keep it and re-place it after Communication Patterns |
| `crates/buzz-acp/src/lib.rs` | machine-check the shared prompt contract; **one** idle reaper (#189 compose of #5682 + #169) feeding Ready → Draining → Listening | retain prompt assertions; keep a single `idle_pool_sleep_reaper` that calls `enter_draining` — never a second top-of-loop timer or sync teardown path |
| `crates/buzz-acp/src/pool.rs` | resume-first session acquire + ledger declare-at-birth (#169); post-load lineage check + live rotation persist (#180); #187 path leases + Busy refusal + checkout notice; #188 `CoworkTurnGuard` on Folder checkout | keep resume/rebuild inside the channel session block; path exclusive lease before ensure; Busy is `PromptOutcome::Ok(Refusal)`; Folder arm begins shadow checkpoints after the path lease |
| `crates/buzz-acp/src/pool_lifecycle.rs` | Ready → Draining → Listening reverse transition (#169) | merge drain helpers; preserve forward wake/retry contract tests |
| `crates/buzz-acp/src/session_ledger.rs` | Crew-landed durable session ledger (#169) + rotation/lineage (#180) + owner `compaction_count` / turn net (#173; upstream-candidacy) | additive file — prefer keeping intact on sync |
| `crates/buzz-acp/src/compaction_signal.rs` | Crew-landed honest CompactionSignal adapters + aging projection (#173) | additive file — prefer keeping intact on sync |
| `crates/buzz-acp/src/guided_handover.rs` | Crew-landed owner guided/blind handover controls (#173) | additive file — prefer keeping intact on sync |
| `crates/buzz-acp/src/acp.rs` | ACP v1 `session/load` client (#169); Hermes/Codex rotation signal capture (#180); `_PostCompact` hook count (#173); upstream standard usage tracker (#4950); #190 retain latest `sessionUpdate: plan` | retain Crew elicitation/rotation/hooks/plan snapshot **and** upstream `standard_usage` field sets; do not fold usage into compaction or invent a second plan protocol |
| `crates/buzz-acp/src/declared_plan.rs` | Crew-landed ACP plan parse/retain (#190) | additive file — prefer keeping intact on sync |
| `crates/buzz-acp/src/config.rs` | single idle policy: `--pool-idle-timeout` / `BUZZ_ACP_POOL_IDLE_TIMEOUT` default 1800; alias `--idle-pool-sleep` / `BUZZ_ACP_IDLE_POOL_SLEEP` (#189) | one resolved `pool_idle_timeout_secs`; aliases must not arm a second timer; primary wins when both set |
| `desktop/src-tauri/src/managed_agents/runtime.rs` | desktop injects the same 1800s value for both idle env names on lazy pairs (#169/#189); #188 `BUZZ_ACP_COWORK_HISTORY_DIR`; #196 snapshot path via cowork helper | inject identical values next to `BUZZ_ACP_LAZY_POOL`; keep `apply_session_aging_env` + `apply_cowork_history_env` |
| `desktop/src-tauri/src/managed_agents/reserved_env_keys.rs` | reserve both idle env names + handover/aging keys from persona/user override (#169/#189); #188 `BUZZ_ACP_COWORK_HISTORY_DIR` | keep both idle keys in the reserved list; they alias one harness field |
| `crates/buzz-cli/src/commands/messages.rs` | CLI contract tests pin the existing message-build seam | keep tests local to the command module; preserve upstream send behavior |
| `crates/buzz-cli/src/lib.rs` | expose the Crew evidence flag on `messages send` | retain the additive clap field; preserve upstream command variants and help text |
| `crates/buzz-cli/src/commands/mod.rs` | register the Crew-owned evidence kind module | retain the module declaration; do not move validation into upstream command code |
| `crates/buzz-cli/src/commands/evidence.rs` | Crew-owned exact evidence-kind parsing and tag construction | keep canonical wire strings and enum-only validation in this module |
| `crates/buzz-cli/TESTING.md` | document the additive evidence flag in the CLI test inventory | retain the one-row flag inventory update; do not rewrite unrelated runbook steps |
| `desktop/playwright.config.ts` | register Crew evidence contracts + #174 worktree-storage smoke + #175 cross-check + #187 selector spec + #188 Cowork Versions spec + #186 workbench spec + #203 work-tree sidebar | retain the narrow test-match additions; do not reorder unrelated entries |
| `desktop/src/features/messages/lib/projectThreadGitHubStore.ts` | reload generation for live evidence↔CI badge recompute (#175) | keep additive reload helper beside existing cache reset |
| `desktop/src/testing/e2eBridge.ts` | mock storage snapshot + reclaim (#174); `__BUZZ_E2E_SET_THREAD_GITHUB_BY_BRANCH__` (#175); #187 `probe_project_git_workspace` + extra project events; #188 Cowork Versions commands via `e2eCoworkVersions.ts` | keep additive cases; default fixture is self-contained |
| `desktop/src/features/settings/ui/SettingsPanels.tsx` | Settings → Storage section registration (#174) | keep `"storage"` arm + nav descriptor; card lives in Crew-owned feature module |
| `desktop/src/features/settings/ui/SettingsView.tsx` | Personal nav entry for Storage (#174) | retain one-line `"storage"` nav add next to local-archive |
| `desktop/src/app/useAppShellLifecycleEffects.ts` | upstream foreground-ready scheduler (#5696) + #164 workspace snapshot refresh + #174 alive heartbeat | attach Crew hooks beside `useForegroundQueryRefresh`; do not turn app absence into observed idle |
| `desktop/src/app/routes/projects.$projectId.tsx` | #188 `thread` search + Cowork Versions gate | keep GitHub ProjectDetailScreen for git; Cowork screen is Crew-owned |
| `desktop/src/features/agents/observerRelayStore.ts` | upstream envelope batching (#5680) + Crew identity/session_aging/control_result | one `notifyListeners` per envelope; Crew variants live in `observerRelayStoreCrew.ts` under D-022 |
| `desktop/src-tauri/src/commands/project_worktree_details.rs` | `branch_is_pushed` helper for Hibernate tier (#174) | retain pub(crate) helper beside ahead/behind; no mutation path changes |
| `desktop/src-tauri/src/commands/mod.rs` / `lib.rs` | register #174 storage commands + #187 `probe_project_git_workspace` + #188 Cowork Versions commands | retain module + invoke_handler entries only |
| `desktop/src/features/messages/ui/MessageRow.tsx` | pass evidence-card review props through the existing default-body seam (987 lines) | retain the seven-line prop pass-through only; keep evidence logic out of this upstream-derived file |
| `desktop/src/features/messages/ui/MessageThreadPanel.tsx` | #190 declared-plans rail mount via Crew `ThreadPanelDeclaredPlansBody` | keep the one wrapper component; rail implementation stays in Crew-owned files |
| `desktop/src/features/messages/ui/MessageRowDefaultBody.tsx` | dispatch known Crew evidence tags + handover note card before ordinary Markdown rendering (#121/#173) | preserve ordinary body fallback and keep card implementation in Crew-owned files |
| `crates/buzz-acp/src/pool.rs` | persist rotation (#180) + compaction/turn aging emit (#173) after turns | keep additive persist/emit helpers; do not invent parallel session maps |
| `crates/buzz-acp/src/lib.rs` | idle pool + observer controls; guided_handover / blind_session_reset (#173) | retain prompt assertions; keep control arms additive |
| `desktop/src-tauri/src/managed_agents/runtime.rs` | desktop passes pool idle timeout (#169); #173 aging env delegated to `session_aging_env.rs` | keep one-line `apply_session_aging_env` call next to `BUZZ_ACP_LAZY_POOL` |
| `desktop/src-tauri/src/managed_agents/session_aging_env.rs` | Crew-landed handover/aging spawn env (#173) | additive file — prefer keeping intact on sync |
| `desktop/src-tauri/src/managed_agents/reserved_env_keys.rs` | reserve pool idle timeout (#169) + handover/aging keys (#173) | keep keys in the shared reserved list |
| `desktop/src-tauri/src/managed_agents/global_config/mod.rs` | per-app handover summarizer + aging thresholds (#173) | retain additive serde fields with defaults |
| `desktop/src/features/messages/ui/AgentReceiptCard.tsx` | share PR-reference href resolution with the evidence card (173 lines) | retain the existing receipt card behavior; keep the resolver pure and additive |
| `crates/buzz-acp/src/thread_workspace.rs` | #187 `ws`/`base` parse + `plan_thread_worktree` binding arms; #188 `mode=folder` → skip-worktree | keep absent-params identical to today's isolated worktree; Main skips `worktree add`; Folder is a separate query param, not `ws=` |
| `crates/buzz-acp/src/thread_workspace/base.rs` | optional requested base for new worktrees (#187) | default path (`None`) must still fetch remote default / local HEAD |
| `crates/buzz-worktree/src/lib.rs` / `record.rs` / `paths.rs` / `lease.rs` / `path_lease.rs` | LifecycleRecord `base`; path-keyed exclusive leases; `max_last_used_at_for_path` (#187); #196 explicit flock unlock on lease Drop | additive files/fields; do not change root-keyed eviction semantics |
| `desktop/src/features/channels/ui/ChannelPane.tsx` | composer workspace selector wiring (#187) | retain provider + `toolbarExtraActions` only; selector lives in Crew-owned files |
| `desktop/src/features/projects/ui/crew-add-project-flow.tsx` | #188 Cowork accept-at-add (replaces #187 refuse-at-add) | probe then confirm; `isGit: false` opens Cowork copy, not a toast |
| `desktop/src-tauri/src/commands/project_worktree_registry_parse.rs` | classify managed by `.buzz-worktrees` parent, not `buzz/<12hex>` only (#187) | primary stays Main / never GC; shared `ws=branch:` worktrees are Managed |
| `desktop/src-tauri/src/commands/worktree_storage_aggregate.rs` | shared idle = max lastUsedAt across path (#187) | keep #174 suggest-and-confirm; do not GC the canonical checkout |
| `desktop/src-tauri/src/commands/project_worktree_reclaim.rs` | path-lease busy probe (#187) | Busy remains a refusal, not an error |
| `desktop/src/app/routes.ts` | keep `/workbench` + `/workbench/$channelId/$threadRootId` (#186/#219) | routes redirect; do not restore a picker screen |
| `desktop/src/app/routeTree.gen.ts` | TanStack generated tree for the workbench redirect routes (#186/#219) | regenerate on desktop build; do not hand-edit on sync unless the generator is skipped |
| `desktop/src/app/AppShell.helpers.ts` | `/workbench` maps to Inbox or the channel session (#219) | do not restore `selectedView: "workbench"` |
| `desktop/src/app/navigation/useAppNavigation.ts` | `goWorkbench` opens the channel session or Inbox (#219) | keep next to `goProjects`; never navigate to a picker place |
| `desktop/src/app/AppShell.tsx` | no `onSelectProjects` (#223); work-tree thread click opens the channel session (#219) | no sidebar Projects or Workbench destination |
| `desktop/src/features/sidebar/ui/AppSidebar.tsx` | no `onSelectWorkbench` (#219); no `onSelectProjects` / project-folder filter (#223) | keep the union compact (D-022); stream list uses `sidebarRailContract` |
| `desktop/src/features/sidebar/ui/AppSidebarPinnedHeader.tsx` | no Workbench nav (#219); no Projects nav (#223) | do not restyle neighboring entries |
| `desktop/src/features/messages/ui/message-thread-panel-head.tsx` | live-job desk mount (#219) | desk lives in Crew-owned `LiveJobDesk`; no "Open workbench" |
| `desktop/src/features/messages/ui/UnreadDivider.tsx` | optional `label` for workbench catch-up copy (#186) | default remains `"New"`; leftover workbench helpers may still pass `"NEW since you were here"` |
| `desktop/src/features/home/ui/HomeView.tsx` | Mission Inbox opens the channel session (#219) | do not navigate on unverified `row.channelId` |
| `desktop/src/features/home/ui/InboxListPane.tsx` | no workbench hammer pass-through (#219) | Inbox still picks via existing row / channel door |
| `desktop/src/features/home/ui/MissionInboxSections.tsx` | no workbench hammer (#219) | channel door remains |
| `desktop/src-tauri/src/lib.rs` | #196 Tool Pane: `start_background` + `invoke::desktop_invoke_handler!()` (command table extracted so this file stays ≤1000; macro expands at crate root for Tauri `__cmd__*` macros); restore `tauri::Listener` for macOS `.once` | keep `manage(ResourceGovernorHandle)` + `start_background`; do not move `generate_handler!` into a submodule |
| `desktop/src-tauri/src/invoke.rs` | #196 command table extracted from `lib.rs` | names only; expand the macro from crate root |
| `desktop/src-tauri/src/egress_guard_tests.rs` | #196 invoke table moved to `invoke.rs` | keep `invoke.rs` on the ncryptsec filename allowlist next to `lib.rs` |
| `desktop/src-tauri/src/commands/mod.rs` | #196 `canvas_tooling` module | retain `mod` + `pub use`; commands live in the Crew file |
| `desktop/src-tauri/src/commands/canvas.rs` | #196 `pub(crate)` on `fenced_crew_block` / `ensure_canvas_author` | visibility only; do not move YAML parse here |
| `desktop/src-tauri/src/commands/channels.rs` | #196 archive=erase / delete=`simctl delete` via governor | keep the two `try_state` hooks next to existing channel mutations |
| `desktop/src-tauri/src/shutdown.rs` | #196 `quit_governor` on app quit and signals | keep next to `TerminalSessions::shutdown_all` |
| `desktop/src-tauri/Cargo.toml` | #196 `tauri` `unstable` for child webview | keep the feature; fallback window does not need it |
| `desktop/src-tauri/capabilities/default.json` | #196 `crew-browser-*` / `tool-pane-*` window labels | additive labels only |
| `desktop/src-tauri/src/managed_agents/cowork_history_env.rs` | #188 history dir + #196 governor snapshot path from the same app-data dir | keep `HISTORY_DIR_ENV`; snapshot inject stays next to it |
| `desktop/src-tauri/src/managed_agents/reserved_env_keys.rs` | #196 `BUZZ_SIMULATOR_UDID` / `BUZZ_DEV_SERVER_URL` / snapshot path | keep in the reserved list |
| `crates/buzz-acp/src/lib.rs` | #196 `governor_env` module | retain `mod governor_env`; injection stays in `pool.rs` |
| `crates/buzz-acp/src/pool.rs` | #196 per-session `BUZZ_SIMULATOR_UDID` / `BUZZ_DEV_SERVER_URL` | bind `channel_key` before `env_for_channel`; extras go on every MCP server |
| `desktop/playwright.config.ts` | #196 `tool-pane.spec.ts` smoke match | retain the glob; do not reorder unrelated entries |
| `desktop/src/testing/e2eBridge.ts` | #196 Tool Pane commands via `e2eToolPane.ts` | keep `isToolPaneCommand` delegate; do not grow the switch |
| `desktop/src/app/AppShell.tsx` | #196 `useToolPaneShortcuts` | one hook call next to other shortcut mounts |
| `desktop/src/shared/lib/keyboard-shortcuts.ts` | #196 ⇧⌘B Browser / ⇧⌘M Simulator | additive shortcut entries |
| `desktop/src/features/channels/ui/ChannelPane.tsx` | #196 Tool Pane mount when tools open and not in focus drawer | pane lives in Crew-owned `features/tool-pane` |
| `desktop/src/features/channels/ui/ChannelScreenHeader.tsx` | #196 Tools header button next to Term | one component; not on DMs |
| `desktop/src/features/channels/ui/FocusThreadDrawer.tsx` | #196 `channelId` / `threadRootId` pass-through | props only; tabs live in `ChannelToolPane` |
| `crates/buzz-core/src/kind.rs` | #198 `KIND_ORG_ROSTER` = 30680 | keep the constant + ALL_KINDS + parameterized-replaceable assert |
| `crates/buzz-core/src/lib.rs` | #198 `pub mod org_roster` | retain the module line |
| `crates/buzz-relay/src/handlers/mod.rs` | #198 `pub mod org_roster` | retain the module line |
| `crates/buzz-relay/src/handlers/ingest.rs` | #198 UsersWrite + global-only + pre-storage validate | keep next to `KIND_PROJECT`; do not invent a post-insert side effect |
| `crates/buzz-acp/src/lib.rs` | #198 roster cache on PromptContext + inbound handoff gate | keep `mod org_roster`; gate after `author_allowed` |
| `crates/buzz-acp/src/pool.rs` | #198 roster cache hooks; **#233** ORG-CHECK/budget inert (D-069) | must not hold a worktree lease; do not revive Org teaching |
| `crates/buzz-cli/src/lib.rs` | #198 `org` subcommand; **#233** Show/Tree/Publish + `--handoff` error | Portfolio/Steward ok; do not revive chart CLI |
| `crates/buzz-cli/src/commands/mod.rs` | #198 `pub mod org` | retain the module line |
| `crates/buzz-cli/src/commands/messages.rs` | #233 `--handoff` / `--goal` hidden + Usage error | do not restore crew-handoff teaching |
| `desktop/src/shared/constants/kinds.ts` | #198 `KIND_ORG_ROSTER` | keep next to project kinds |
| `mobile/lib/shared/relay/nostr_models.dart` | #198 `orgRoster = 30680` | mirror desktop; no mobile org UI |
| `desktop/src/app/routes.ts` | #198 `/org` route kept for redirect | **#233** `org.tsx` redirects to `/` — do not revive OrgScreen |
| `desktop/src/app/routeTree.gen.ts` | #198 generated `/org` | regenerate on desktop build |
| `desktop/src/app/AppShell.helpers.ts` | **#233** no `"org"` AppView | do not re-add Org view |
| `desktop/src/app/navigation/useAppNavigation.ts` | **#233** `goOrg` removed | do not revive |
| `desktop/src/app/AppShell.tsx` | **#233** `onSelectOrg` removed | do not revive |
| `desktop/src/features/sidebar/ui/AppSidebar.tsx` | **#233** no Org nav | do not revive |
| `desktop/src/features/sidebar/ui/AppSidebarPinnedHeader.tsx` | **#233** Org menu item removed | do not revive |
| `desktop/src/features/communities/useCommunityInit.ts` | **#233** `resetOrgRosterProjection` removed | do not revive Org store |
| `desktop/src/features/messages/ui/MessageRowDefaultBody.tsx` | **#233** no MessageOrgDecorations | do not wrap Org chrome |
| `desktop/src/features/channels/ui/ChannelPane.tsx` | **#233** HandoffAssignControl removed | do not revive |
| `desktop/src/features/messages/hooks.ts` | #198 optional `extraTags` on send | extraTags use `relayClient.sendMessage`; do not grow Tauri `messages.rs` |
| `desktop/src/features/channels/useChannelPaneHandlers.ts` | **#233** pending handoff consume removed | do not revive |
| `desktop/src/features/home/lib/missionInbox.ts` | **#233** no escalationHop | do not revive roster hops |
| `desktop/src/features/home/ui/MissionInboxSections.tsx` | **#233** no officer roster filter | keep Needs-You without Org |
| `desktop/src/features/home/useMissionInboxSections.ts` | **#233** no roster hop annotate | projection only |
| `desktop/src/testing/e2eBridge.ts` | **#233** e2eOrgRoster hooks removed | do not revive chart mocks |
| `desktop/playwright.config.ts` | **#233** `org-removed.spec.ts` (not org-hierarchy) | retain the removal smoke |
| `desktop/src/features/messages/ui/threadPrHub/ThreadFocusForgeSplit.tsx` | #196 PR hub is a sibling Tool Pane tab | compose with `ChannelToolPane`; do not start Workbench |
| `desktop/src/features/sidebar/ui/SidebarSection.tsx` | #196 resource dots on channel rows | `SidebarResourceDots`; no numbers, no motion |
| `desktop/src/features/settings/ui/SettingsPanels.tsx` | #196 `"devices-preview"` arm | card lives in Crew-owned feature module |
| `desktop/src/features/settings/ui/SettingsView.tsx` | #196 Devices & Preview nav | one Personal nav entry |
| `desktop/src/features/communities/useCommunityInit.ts` | #196 reset governor + tool pane stores; #197 reset agent-control store | add next to other community-scoped resets |
| `desktop/tests/helpers/settings.ts` | #196 `"devices-preview"` section id | keep in the settings helper union |
| `desktop/src-tauri/src/lib.rs` | #197 `agent_control` module + localhost server spawn | keep next to media proxy / governor `start_background` |
| `desktop/src-tauri/src/invoke.rs` | #197 `agent_control_*` commands | additive generate_handler entries only |
| `desktop/src-tauri/src/managed_agents/runtime.rs` | #197 inject control URL/token at spawn | keep next to cowork/snapshot env; must not take worktree leases |
| `desktop/src-tauri/src/managed_agents/reserved_env_keys.rs` | #197 `BUZZ_DESKTOP_CONTROL_*` / thread root | keep in the reserved list |
| `desktop/src-tauri/src/commands/canvas_tooling.rs` | #197 `browserAllowlist` on `tooling` | additive optional field |
| `crates/buzz-core/src/kind.rs` | #197 `KIND_AGENT_INSTRUMENT_OVERLAY` = 24201 | keep in `ALL_KINDS` + `P_GATED_KINDS` |
| `desktop/src/shared/constants/kinds.ts` | #197 overlay kind 24201 | keep next to other NIP-AO kinds |
| `mobile/lib/shared/relay/nostr_models.dart` | #197 `agentInstrumentOverlay` | keep kinds in sync |
| `crates/buzz-acp/src/lib.rs` | #197 `desktop_control` + MCP env forward | retain `mod desktop_control`; no worktree I/O |
| `crates/buzz-acp/src/pool.rs` | #197 lease.release on turn end; thread-root MCP env | keep next to `TurnCompletionGuard` / `mcp_servers_with_git_origin` |
| `crates/buzz-dev-mcp/src/lib.rs` | #197 14 `#[tool]` methods | wrappers only; impl lives in `desktop_tools.rs` |
| `desktop/src/features/channels/ui/ChannelPane.tsx` | #197 origin prompt when pane closed | keep `ChannelOriginPrompt` next to user-input stack |
| `desktop/src/testing/e2eBridge.ts` | #197 `__BUZZ_E2E_SET_AGENT_CONTROL__` | keep next to governor seed; logic in `e2eToolPane.ts` |
| `desktop/playwright.config.ts` | #197 `agent-desktop-tools.spec.ts` smoke match | retain the glob |
| `crates/buzz-core/src/kind.rs` | #200 `KIND_REPO_WIKI_PAGE` = 30623 | keep the constant + ALL_KINDS + parameterized-replaceable assert |
| `crates/buzz-core/src/lib.rs` | #200 `pub mod wiki_page` | retain the module line |
| `crates/buzz-relay/src/handlers/mod.rs` | #200 `pub mod wiki_page` | retain the module line |
| `crates/buzz-relay/src/handlers/ingest.rs` | #200 ReposWrite + `validate_roster_or_wiki` | keep next to git kinds; wiki validate stays extracted (D-022) |
| `crates/buzz-dev-mcp/src/lib.rs` | #200 wiki `#[tool]` + argv0 `crew-wiki` | wrappers only; impl lives in `wiki_tools.rs` |
| `crates/buzz-dev-mcp/src/shim.rs` | #200 `crew-wiki` multicall name | keep next to `rg`/`tree`/`buzz` |
| `crates/buzz-cli/src/links.rs` | #200 `file_link` | keep format sibling of repo/pr/issue |
| `desktop/src/shared/constants/kinds.ts` | #200 `KIND_LONG_FORM` + `KIND_REPO_WIKI_PAGE` | keep next to git kinds |
| `mobile/lib/shared/relay/nostr_models.dart` | #200 `longForm` / `repoWikiPage` | mirror desktop; no mobile wiki UI in this PR |
| `desktop/src/app/routes.ts` | #200 `/wiki` | one `route()` line next to org |
| `desktop/src/app/routeTree.gen.ts` | #200 generated `/wiki` | regenerate on desktop build |
| `desktop/src/app/AppShell.helpers.ts` | #200 `AppView` + `deriveShellRoute` include `"wiki"` | one union member + one pathname arm |
| `desktop/src/app/navigation/useAppNavigation.ts` | #200 `goWiki` | keep next to `goOrg` |
| `desktop/src/app/AppShell.tsx` | #200 `onSelectWiki` | one callback pass-through |
| `desktop/src/features/sidebar/ui/AppSidebar.tsx` | #200 `onSelectWiki` prop | keep the union compact (D-022); props live in `AppSidebarProps.ts` |
| `desktop/src/features/sidebar/ui/AppSidebarPinnedHeader.tsx` | #200 Wiki nav item after Org | one `SidebarMenuItem` |
| `desktop/src/features/communities/useCommunityInit.ts` | #200 `resetWikiStore` | one call in `resetCommunityState` |
| `desktop/src/shared/lib/entityLink.ts` | #200 `buzz://file` host | keep next to pr/issue/repo; file is inline, not a preview card |
| `desktop/src/shared/ui/markdown/utils.ts` | #200 `buzzDeepLinkUrlTransform` preserves valid file links | `parseEntityLink` is the allowlist |
| `desktop/src/shared/ui/markdown/CodeBlock.tsx` | #200 mermaid fence → `MarkdownMermaid` | keep the language arm; renderer lives in Crew file |
| `desktop/src/shared/ui/markdown/entityLinks.tsx` | #200 `file` case opens file panel | exhaustive switch |
| `desktop/src/features/projects/ui/ProjectWorkspaceTabList.tsx` | #200 Wiki tab | one tab next to Files |
| `desktop/src/features/projects/ui/ProjectWorkspaceTabs.tsx` | #200 wiki `TabsContent` + pending file-open | tab body lives in Crew `WikiProjectTab` |
| `desktop/src/features/projects/ui/ProjectRepositoryPanel.tsx` | #200 consume pending wiki file open | do not reset selection when `filesKey` is unchanged |
| `desktop/src-tauri/src/lib.rs` | #200 `mod wiki_worker` + `wiki_generate` import | keep next to other command modules |
| `desktop/src-tauri/src/invoke.rs` | #200 `wiki_generate` | additive generate_handler entry only |
| `desktop/src-tauri/Cargo.toml` | #200 `crew-wiki` path dep | keep the path dep |
| `desktop/src/testing/e2eBridge.ts` | #200 wiki mock via `e2eWiki.ts` | keep additive REQ/EVENT + window hooks |
| `desktop/playwright.config.ts` | #200 `crew-wiki.spec.ts` smoke match | retain the glob |
| `migrations/0008_fresh_install_search_allowlist.sql` | #200 30023 + 30623 on fresh-install allowlist | do not add 30623 to the privacy exclusion CASE |
| `scripts/maintenance/nip_rs_search_allowlist.sql` | #200 30023 + 30623 | same allowlist; exclusion CASE untouched |

## Before feature work

```bash
git status --short --branch
git fetch --prune origin
git fetch --prune upstream
git switch main
git pull --ff-only origin main
```

If `main` is not clean, stop. Do not hide or discard local changes.

## Sync procedure

```bash
git fetch --prune upstream
git fetch --prune origin
git switch main
git pull --ff-only origin main
git switch -c sync/upstream-YYYY-MM-DD
git merge --no-edit upstream/main
```

Then:

1. Record the chosen Buzz version, tag, and exact merged commit in
   `upstream-buzz.json`.
2. Inspect every conflict as a fork-maintenance signal.
3. Prefer moving Crew behavior into additive files over repeatedly resolving
   the same upstream file. If the Desktop file-size ratchet trips on a shared
   file, extract Crew-owned additions into Crew-only files so the shared file
   returns to at or below the upstream line count (D-022) — do not raise the
   limit or grant a sync-only exception. For a file with a recorded baseline,
   its recorded `lines` value uses `wc -l` semantics: update only that value to
   the exact count printed by the guard in the same sync PR when upstream grows
   it; Crew-owned growth still requires extraction under D-022.
4. After the merge, run `gh workflow list --all` and disable any newly imported
   workflows that fall outside Crew scope (for example Sprig publication).
5. Run focused tests for conflict areas.
6. Run upstream's required quality gates.
7. Review the fork delta:

```bash
git diff --stat upstream/main...HEAD
git diff --name-status upstream/main...HEAD
```

1. Push the sync branch and merge it through a reviewed PR into Crew `main`.

Run the manual `NuncioCrew Upstream Sync` workflow on the sync branch before
merge. Normal feature PRs intentionally do not run the inherited multi-product
Buzz matrix; see [`CI.md`](CI.md).

In Actions, select the sync branch instead of accepting the default `main`
branch. The CLI equivalent is:

```bash
gh workflow run nuncio-crew-upstream-sync.yml \
  --ref sync/upstream-YYYY-MM-DD
```

Confirm the resulting run's head SHA equals the sync branch HEAD before using
it as compatibility evidence.

## Feature branches

Use short-lived, area-prefixed names, for example:

```text
docs/crew-foundation
project/local-workspace-location
board/event-projection
agents/project-context
sync/upstream-2026-07-30
```

Branch names describe durable product areas, not plan phase numbers or audit
codes.

## Conflict policy

When upstream changes one of Crew's few edited files:

1. Re-read the upstream intent.
2. Reapply only the smallest Crew hook.
3. Move new logic into Crew-owned files.
4. Run the relevant upstream test and Crew contract test.
5. Record a decision if the maintenance boundary changes.

Do not resolve conflicts by keeping "ours" wholesale.

## Fork-drift review

After every upstream sync, classify the delta:

| Class                                 | Expected action                            |
| ------------------------------------- | ------------------------------------------ |
| Added Crew file                       | Normal                                     |
| Route/nav hook                        | Keep tiny and inspect manually             |
| Upstream file with repeated conflicts | Redesign integration boundary              |
| Rust modification                     | Revalidate the spike evidence and approval |
| Copied upstream implementation        | Remove or replace with composition         |

The goal is not a zero diff. The goal is a small, understandable, replayable
diff whose maintenance cost is visible.

## Recovery

If a sync branch becomes confused, do not rewrite or reset a dirty user
checkout. Preserve the branch, create a fresh sync branch from clean `main`,
and compare the attempts. Destructive Git recovery requires explicit approval.

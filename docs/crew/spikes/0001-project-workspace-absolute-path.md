# Spike 0001 — Project Workspace Through Absolute Path

- **Status:** PASS with documented limits
- **Date:** 2026-07-30

## Question

Can an agent work in a Project's absolute local directory while
`session/new.cwd` remains the shared Buzz working directory?

## Decision affected

Whether the first Project slice can remain a TypeScript/context change or must
change Rust to create per-Project ACP cwd.

## Hypothesis

Project-channel context can provide an absolute path. Providers can read and
write there through their existing ACP/tool path without changing session cwd.

## Scope

Providers:

- Codex CLI `0.145.0` through `codex-acp`;
- Claude Code `2.1.220` through `claude-agent-acp`;
- Cursor Agent `2026.07.23-e383d2b` through native ACP;
- Devin through native ACP.

The test used authenticated subscription-backed local CLIs. No Buzz production
code, relay data, or user repository file was modified.

## Exclusions

- repository-local instruction discovery;
- relative-path behavior;
- resumed provider sessions;
- Windows paths;
- mobile access;
- multiple machines.

## Pass criteria

With session cwd and Project directory in different real locations, each
provider must:

1. Read a nonce from the Project directory by absolute path.
2. Create a result file inside that directory.
3. Read the result back.
4. Preserve session cwd.
5. Produce independently verified file contents.

## Fail criteria

- external write is denied with no supported existing tool path;
- provider changes or copies the Project;
- result exists only in session cwd;
- success is claimed without filesystem evidence.

## Method

Two isolated directories were created:

- session cwd under `/tmp`;
- Project fixture under the real local workspace tree.

The ACP client mirrored Buzz behavior:

- protocol initialization and `session/new`;
- automatic selection of `allow_once` permission;
- `buzz-dev-mcp` attached for Codex;
- provider-native ACP for Cursor and Devin;
- `bypassPermissions` selected for Claude when advertised.

Each provider received the same absolute-path contract. Output files were
checked independently for exact content and line termination.

## Results

| Provider    | Native/direct result                       | ACP result | Tool path                                               |
| ----------- | ------------------------------------------ | ---------- | ------------------------------------------------------- |
| Codex       | Read passed; external native write blocked | PASS       | `buzz-dev-mcp` `read_file` and `shell(workdir=Project)` |
| Claude Code | PASS                                       | PASS       | Native Read/Write in `bypassPermissions`                |
| Cursor      | PASS                                       | PASS       | Native ACP Read/Edit/Shell                              |
| Devin       | Not needed for conclusion                  | PASS       | Native ACP Read/Write with `allow_once`                 |

Devin initially omitted the final newline despite correct logical content. A
second run explicitly required it and byte-level verification passed.

Unrelated provider warnings about optional MCP authentication and imported hook
formats did not affect Project file access.

## Source evidence

- `crates/buzz-acp/src/lib.rs` captures process current directory in
  `PromptContext`.
- `crates/buzz-dev-mcp/src/paths.rs` explicitly does not enforce containment.
- `crates/buzz-dev-mcp/src/shell.rs` accepts a per-call `workdir` and sets the
  child process current directory.
- Upstream Project announcements already use `buzz-channel` on kind `30617`.

## Limitations

This spike proves explicit absolute-path work. It does not prove that:

- provider-native tools always choose the correct path without instruction;
- repository-local `AGENTS.md` or `CLAUDE.md` is discovered;
- all relative commands run in the Project;
- a future provider sandbox behaves identically;
- local paths are safe to publish to every relay configuration.

## Verdict

The first Project slice does not require a Rust cwd change.

Project-channel context must include the absolute path. Codex instructions must
direct filesystem and Git work through `buzz-dev-mcp` with the Project
`workdir` when native workspace restrictions apply.

## Follow-up test contract

Before implementation, write RED tests for:

- stable NIP-34 identity when local path changes;
- local-location tag round-trip;
- coexistence with clone and relay tags;
- multiple locations;
- missing and inaccessible directory;
- path containing spaces and Unicode;
- privacy behavior for non-local relay targets.

## Cleanup

All Project fixtures and temporary ACP harness files were removed. The local
workspace was verified clean after the spike.

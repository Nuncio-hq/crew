import assert from "node:assert/strict";
import { test } from "node:test";

import {
  AGENT_MENTION_AVATAR_VAR,
  agentMentionAvatarDecoration,
  agentMentionAvatarStyle,
} from "./agentMentionAvatar.ts";
import { AGENT_MENTION_AVATAR_CLASS } from "./mentionChip.ts";
import { buildAnimatedAvatarUrl } from "../lib/animatedAvatar.ts";

test("agentMentionAvatarStyle: blank / null / undefined → empty (robot fallback)", () => {
  assert.deepEqual(agentMentionAvatarStyle(null), {});
  assert.deepEqual(agentMentionAvatarStyle(undefined), {});
  assert.deepEqual(agentMentionAvatarStyle(""), {});
  assert.deepEqual(agentMentionAvatarStyle("   "), {});
});

test("agentMentionAvatarStyle: plain URL becomes url() custom property", () => {
  const result = agentMentionAvatarStyle(
    "https://cdn.example/avatars/agent.png",
  );
  assert.equal(result.className, AGENT_MENTION_AVATAR_CLASS);
  assert.equal(
    result.style?.[AGENT_MENTION_AVATAR_VAR],
    'url("https://cdn.example/avatars/agent.png")',
  );
});

test("agentMentionAvatarStyle: animated avatar uses poster frame only", () => {
  const animated = buildAnimatedAvatarUrl(
    "https://cdn.example/poster.png",
    "https://cdn.example/anim.png",
  );
  const result = agentMentionAvatarStyle(animated);
  assert.equal(
    result.style?.[AGENT_MENTION_AVATAR_VAR],
    'url("https://cdn.example/poster.png")',
  );
});

test("agentMentionAvatarStyle: escapes quotes and backslashes in URL", () => {
  const result = agentMentionAvatarStyle('https://cdn.example/a"b\\c.png');
  assert.equal(
    result.style?.[AGENT_MENTION_AVATAR_VAR],
    'url("https://cdn.example/a\\"b\\\\c.png")',
  );
});

test("agentMentionAvatarDecoration: serializes style as CSS declaration", () => {
  const decoration = agentMentionAvatarDecoration(
    "https://cdn.example/avatars/agent.png",
  );
  assert.ok(decoration);
  assert.equal(decoration.className, AGENT_MENTION_AVATAR_CLASS);
  assert.equal(
    decoration.style,
    '--agent-mention-avatar:url("https://cdn.example/avatars/agent.png")',
  );
});

test("agentMentionAvatarDecoration: null when no avatar", () => {
  assert.equal(agentMentionAvatarDecoration(null), null);
  assert.equal(agentMentionAvatarDecoration(""), null);
});

test("agentMentionAvatarStyle: runtime public faces paint mention chips", () => {
  for (const url of [
    "/harness-logos/hermes.png",
    "/harness-logos/claude.png",
    "/harness-logos/goose.svg",
    "/harness-logos/cursor.svg",
    "/harness-logos/terminal.svg",
  ]) {
    const result = agentMentionAvatarStyle(url);
    assert.equal(result.className, AGENT_MENTION_AVATAR_CLASS);
    assert.equal(result.style?.[AGENT_MENTION_AVATAR_VAR], `url("${url}")`);
  }
});

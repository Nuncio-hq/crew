import assert from "node:assert/strict";
import test from "node:test";

import {
  getFailedAdapterRepairWarning,
  getInstallErrorMessage,
  getInstallOutcomeMessages,
} from "./installError.ts";

test("getInstallErrorMessage: empty steps array returns fallback", () => {
  assert.equal(getInstallErrorMessage([]), "Install failed with no output.");
});

test("getInstallErrorMessage: failed step without hint contains step name and stderr", () => {
  const message = getInstallErrorMessage([
    {
      step: "adapter",
      command: "npm install -g @block/buzz-acp",
      success: false,
      stdout: "",
      stderr: "EACCES: permission denied",
      exitCode: 1,
    },
  ]);
  assert.match(message, /Step "adapter" failed:/);
  assert.match(message, /EACCES: permission denied/);
});

test("getInstallErrorMessage: failed step without hint does not contain hint-ish text", () => {
  const message = getInstallErrorMessage([
    {
      step: "adapter",
      command: "npm install -g @block/buzz-acp",
      success: false,
      stdout: "",
      stderr: "EACCES: permission denied",
      exitCode: 1,
    },
  ]);
  assert.doesNotMatch(message, /npm config set prefix/);
});

test("getInstallErrorMessage: failed step with hint starts with hint and still contains stderr", () => {
  const hint =
    "Fix the npm prefix ownership:\n  sudo chown -R $USER $(npm config get prefix)";
  const message = getInstallErrorMessage([
    {
      step: "adapter",
      command: "npm install -g @block/buzz-acp",
      success: false,
      stdout: "",
      stderr: "EACCES: permission denied, mkdir '/usr/local/lib'",
      exitCode: 1,
      hint,
    },
  ]);
  assert.ok(message.startsWith(hint), "message should start with hint");
  assert.match(message, /EACCES: permission denied/);
});

test("getInstallErrorMessage: failed step with empty stderr falls back to stdout", () => {
  const message = getInstallErrorMessage([
    {
      step: "node",
      command: "node --version",
      success: false,
      stdout: "some stdout output",
      stderr: "",
      exitCode: 1,
    },
  ]);
  assert.match(message, /some stdout output/);
});

test("getInstallErrorMessage: hint and step detail are separated by double newline for whitespace-pre-line rendering", () => {
  const hint = "Git Bash is required. Install it from git-scm.com.";
  const message = getInstallErrorMessage([
    {
      step: "shell",
      command: "bash -l -c 'npm install'",
      success: false,
      stdout: "",
      stderr: "bash: command not found",
      exitCode: 127,
      hint,
    },
  ]);
  assert.ok(
    message.includes("\n\n"),
    "hint and step detail should be separated by a blank line",
  );
  assert.ok(message.startsWith(hint));
});

test("getInstallErrorMessage: only reports the last (failing) step when multiple steps present", () => {
  const message = getInstallErrorMessage([
    {
      step: "node",
      command: "node --version",
      success: true,
      stdout: "v20.0.0",
      stderr: "",
      exitCode: 0,
    },
    {
      step: "adapter",
      command: "npm install -g @agentclientprotocol/claude-code-acp",
      success: false,
      stdout: "",
      stderr: "npm ERR! code E404",
      exitCode: 1,
    },
  ]);
  assert.match(message, /Step "adapter" failed:/);
  assert.match(message, /npm ERR! code E404/);
  assert.doesNotMatch(message, /Step "node"/);
});

const failedAdapterRepairHint =
  "Claude Code adapter was removed during architecture repair and could not be reinstalled; open Settings → Agent runtimes and click Install for Claude Code.";

test("getFailedAdapterRepairWarning: success:true + failed adapter-repair => warning visible", () => {
  const warning = getFailedAdapterRepairWarning([
    {
      step: "adapter",
      command: "npm install -g @openai/codex",
      success: true,
      stdout: "",
      stderr: "",
      exitCode: 0,
    },
    {
      step: "adapter-repair",
      command: "npm install -g @anthropic/claude",
      success: false,
      stdout: "",
      stderr: "npm ERR! network",
      exitCode: 1,
      hint: failedAdapterRepairHint,
    },
  ]);
  assert.equal(warning, failedAdapterRepairHint);
});

test("getFailedAdapterRepairWarning: no failed adapter-repair => null", () => {
  assert.equal(
    getFailedAdapterRepairWarning([
      {
        step: "adapter",
        command: "npm install -g @openai/codex",
        success: true,
        stdout: "",
        stderr: "",
        exitCode: 0,
      },
      {
        step: "adapter-repair",
        command: "npm install -g @anthropic/claude",
        success: true,
        stdout: "",
        stderr: "",
        exitCode: 0,
      },
    ]),
    null,
  );
});

test("getInstallOutcomeMessages: primary success with failed sibling repair is warning not error", () => {
  const outcome = getInstallOutcomeMessages({
    success: true,
    steps: [
      {
        step: "adapter",
        command: "npm install -g @openai/codex",
        success: true,
        stdout: "",
        stderr: "",
        exitCode: 0,
      },
      {
        step: "adapter-repair",
        command: "reinstall after architecture repair",
        success: false,
        stdout: "",
        stderr: "managed Node.js runtime is not ready",
        exitCode: null,
        hint: failedAdapterRepairHint,
      },
    ],
  });
  assert.equal(outcome.error, null);
  assert.equal(outcome.warning, failedAdapterRepairHint);
});

test("getInstallOutcomeMessages: hard failure stays error (not warning)", () => {
  const outcome = getInstallOutcomeMessages({
    success: false,
    steps: [
      {
        step: "adapter",
        command: "npm install -g @openai/codex",
        success: false,
        stdout: "",
        stderr: "EACCES",
        exitCode: 1,
      },
    ],
  });
  assert.match(outcome.error ?? "", /EACCES/);
  assert.equal(outcome.warning, null);
});

import assert from "node:assert/strict";
import { registerHooks } from "node:module";
import test from "node:test";
registerHooks({
  resolve(specifier, context, nextResolve) {
    return specifier === "@/shared/api/tauri"
      ? { shortCircuit: true, url: "probe-stub:tauri" }
      : nextResolve(specifier, context);
  },
  load(url, context, nextLoad) {
    return url === "probe-stub:tauri"
      ? {
          shortCircuit: true,
          format: "module",
          source:
            "export const invokeTauri = async (command,input) => {globalThis.__PROBE_INPUT__={command,input}; return globalThis.__PROBE_RESPONSE__;}",
        }
      : nextLoad(url, context);
  },
});
const { probeProjectGitWorkspace } = await import(
  "./projectGitWorkspaceProbe.ts"
);
test("probe preserves native selected folder versus Git ancestor without changing the submitted path", async () => {
  globalThis.__PROBE_RESPONSE__ = {
    isGit: true,
    selectedPath: "/work/Project/sub",
    gitRoot: "/work/Project",
    selection: "subdirectory",
  };
  const result = await probeProjectGitWorkspace("/work/Project/sub");
  assert.equal(globalThis.__PROBE_INPUT__.input.path, "/work/Project/sub");
  assert.equal(result.selectedPath, "/work/Project/sub");
  assert.equal(result.gitRoot, "/work/Project");
  assert.equal(result.selection, "subdirectory");
});
test("older native responses cannot certify an exact root or this-device folder access", async () => {
  globalThis.__PROBE_RESPONSE__ = { isGit: true };
  const result = await probeProjectGitWorkspace("/work/Project");
  assert.equal(result.selectedPath, null);
  assert.equal(result.gitRoot, null);
  assert.equal(result.selection, "unknown");
});

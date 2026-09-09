import assert from "node:assert/strict";
import { spawnSync } from "node:child_process";
import {
  mkdtempSync,
  mkdirSync,
  readFileSync,
  writeFileSync,
  realpathSync,
  rmSync,
  existsSync,
  readlinkSync,
} from "node:fs";
import { tmpdir, homedir } from "node:os";
import { dirname, resolve, join, sep } from "node:path";
import { fileURLToPath } from "node:url";
import test from "node:test";

const source = resolve(dirname(fileURLToPath(import.meta.url)), "../../..");
const wrapper = join(source, "scripts/hook-lane-wrapper.sh");
const yaml = readFileSync(join(source, "lefthook.yml"), "utf8");
const quote = (s) => `'${s.replaceAll("'", "'\\''")}'`;

function graph(t) {
  const root = realpathSync(
    mkdtempSync(join(tmpdir(), "crew hook isolation ")),
  );
  t.after(() => rmSync(root, { recursive: true, force: true }));
  function path(...parts) {
    const p = resolve(root, ...parts);
    assert.ok(
      p.startsWith(root + sep),
      "fixture destination must stay in scratch graph",
    );
    return p;
  }
  const home = path("home");
  mkdirSync(home);
  const env = {
    PATH: "/usr/bin:/bin:/usr/sbin:/sbin",
    HOME: home,
    XDG_CONFIG_HOME: home,
    TMPDIR: root,
    GIT_CONFIG_NOSYSTEM: "1",
    GIT_CONFIG_GLOBAL: path("global config"),
    GIT_TERMINAL_PROMPT: "0",
    LC_ALL: "C",
  };
  writeFileSync(
    env.GIT_CONFIG_GLOBAL,
    "[user]\n name = Fixture\n email = fixture@example.invalid\n[init]\n defaultBranch = main\n",
  );
  function run(cwd, command, args, extra = {}) {
    assert.ok(cwd === root || realpathSync(cwd).startsWith(root + sep));
    return spawnSync(command, args, {
      cwd,
      env: { ...env, ...extra },
      encoding: "utf8",
      timeout: 30000,
      maxBuffer: 1024 * 1024,
    });
  }
  function ok(cwd, command, args, extra) {
    const r = run(cwd, command, args, extra);
    assert.equal(
      r.status,
      0,
      `${command} ${args.join(" ")}: ${r.error || ""}\n${r.stdout}\n${r.stderr}`,
    );
    return r.stdout.trim();
  }
  const git = (cwd, ...args) => ok(cwd, "git", args);
  const main = path("scratch main");
  mkdirSync(main);
  git(main, "init", "-b", "main");
  writeFileSync(join(main, "seed"), "seed");
  git(main, "add", ".");
  git(main, "commit", "-sm", "seed");
  const linked = path("linked wt");
  git(main, "worktree", "add", "-b", "linked", linked);
  const common = join(main, ".git");
  const linkedDir = git(linked, "rev-parse", "--absolute-git-dir");
  const localVars = git(main, "rev-parse", "--local-env-vars").split("\n");
  function snapshot() {
    return {
      config: readFileSync(join(common, "config"), "hex"),
      refs: git(main, "for-each-ref", "--format=%(refname) %(objectname)"),
      files: [
        join(common, "HEAD"),
        join(common, "index"),
        join(linkedDir, "HEAD"),
        join(linkedDir, "index"),
      ].map((p) => readFileSync(p, "hex")),
    };
  }
  // The child deliberately uses raw Git, including the observed no-destination init.
  const lane = path("fixture lane.sh");
  writeFileSync(
    lane,
    `#!/bin/sh\nset -eu\ncase "$1" in ${quote(root)}/*) ;; *) exit 90;; esac\nmkdir -p "$1"\ngit -C "$1" init -b main\ngit -C "$1" config user.name Test\ngit -C "$1" config user.email test@example.com\nprintf fixture > "$1/file"\ngit -C "$1" add file\ngit -C "$1" commit -m fixture\ngit init --bare "$1/bare.git"\n`,
    { mode: 0o755 },
  );
  return {
    root,
    path,
    env,
    run,
    ok,
    git,
    main,
    linked,
    common,
    linkedDir,
    localVars,
    snapshot,
    lane,
  };
}

test("lane isolates real nested Git writes in main and linked checkouts", (t) => {
  const g = graph(t);
  const cases = [
    [g.main, {}],
    [g.main, { GIT_DIR: g.common, GIT_WORK_TREE: g.main }],
    [g.linked, { GIT_DIR: g.linkedDir }],
    [g.linked, { GIT_COMMON_DIR: g.common }],
    [g.linked, { GIT_INDEX_FILE: join(g.common, "index") }],
    [g.linked, { GIT_DIR: g.path("nonexistent") }],
    [
      g.linked,
      {
        GIT_DIR: g.linkedDir,
        GIT_COMMON_DIR: g.common,
        GIT_WORK_TREE: g.main,
        GIT_OBJECT_DIRECTORY: join(g.common, "objects"),
        GIT_INDEX_FILE: join(g.common, "index"),
      },
    ],
    [
      g.linked,
      {
        GIT_CONFIG_COUNT: "1",
        GIT_CONFIG_KEY_0: "core.bare",
        GIT_CONFIG_VALUE_0: "true",
        GIT_CONFIG_PARAMETERS: "'core.bare=true'",
      },
    ],
  ];
  for (const [i, [cwd, env]] of cases.entries()) {
    const before = g.snapshot();
    g.ok(cwd, wrapper, [g.lane, g.path(`nested ${i}`)], env);
    assert.deepEqual(
      g.snapshot(),
      before,
      `caller metadata changed in case ${i}`,
    );
    assert.equal(g.git(g.path(`nested ${i}`), "config", "user.name"), "Test");
  }
});

test("wrapper preserves argv, cwd, streams, build and nonlocal Git environment", (t) => {
  const g = graph(t);
  const probe = g.path("probe.mjs");
  writeFileSync(
    probe,
    `import fs from 'node:fs'; process.stdout.write(JSON.stringify({args:process.argv.slice(2),cwd:process.cwd(),env:process.env,input:fs.readFileSync(0,'utf8')})); process.stderr.write('stderr preserved'); process.exit(23);`,
  );
  const kept = {
    CARGO_BUILD_JOBS: "2",
    GIT_SSH_COMMAND: "transport marker",
    GIT_ASKPASS: "askpass marker",
    GIT_CONFIG_SYSTEM: g.path("system config"),
    GIT_AUTHOR_NAME: "author marker",
  };
  const dirty = Object.fromEntries(
    g.localVars.map((name) => [name, "invalid"]),
  );
  const result = spawnSync(
    wrapper,
    [process.execPath, probe, "", "two words", "*", "--flag", "line\nbreak"],
    {
      cwd: g.linked,
      env: {
        ...g.env,
        ...kept,
        ...dirty,
        GIT_CONFIG_COUNT: "0",
        GIT_CONFIG_PARAMETERS: "",
      },
      encoding: "utf8",
      input: "stdin preserved",
      timeout: 30000,
    },
  );
  assert.equal(result.status, 23, result.stderr);
  const actual = JSON.parse(result.stdout);
  assert.deepEqual(actual.args, [
    "",
    "two words",
    "*",
    "--flag",
    "line\nbreak",
  ]);
  assert.equal(actual.cwd, g.linked);
  assert.equal(actual.input, "stdin preserved");
  assert.equal(result.stderr, "stderr preserved");
  for (const name of g.localVars)
    assert.equal(actual.env[name], undefined, name);
  for (const [name, value] of Object.entries({
    ...kept,
    GIT_CONFIG_GLOBAL: g.env.GIT_CONFIG_GLOBAL,
  }))
    assert.equal(actual.env[name], value, name);
});

test("wrapper fails closed for missing command, outside/root mismatch and bad discovery", (t) => {
  const g = graph(t);
  const marker = g.path("must not run");
  const args = ["touch", marker];
  assert.notEqual(g.run(g.main, wrapper, []).status, 0);
  assert.notEqual(
    g.run(g.root, wrapper, args, { GIT_DIR: g.linkedDir }).status,
    0,
  );
  const sub = g.path("scratch main/sub");
  mkdirSync(sub);
  assert.notEqual(g.run(sub, wrapper, args).status, 0);
  const bin = g.path("stub bin");
  mkdirSync(bin);
  for (const body of [
    "exit 9",
    "printf 'GIT_INDEX_FILE\\n'",
    "printf 'GIT_DIR\\nPATH\\n'",
    "printf 'GIT_DIR\\nGIT_BAD=oops\\n'",
    "printf 'GIT_DIR\\n\\nGIT_INDEX_FILE\\n'",
  ]) {
    writeFileSync(join(bin, "git"), `#!/bin/sh\n${body}\n`, { mode: 0o755 });
    const r = g.run(g.main, wrapper, args, { PATH: `${bin}:${g.env.PATH}` });
    assert.notEqual(r.status, 0);
    assert.equal(existsSync(marker), false);
  }
});

function pinnedLefthook(g) {
  const pin = readlinkSync(join(source, "bin/lefthook"))
    .replace(/^\./, "")
    .replace(/\.pkg$/, "");
  const cache =
    process.env.HERMIT_STATE_DIR ||
    (process.platform === "darwin"
      ? join(homedir(), "Library/Caches/hermit")
      : join(
          process.env.XDG_CACHE_HOME || join(homedir(), ".cache"),
          "hermit",
        ));
  // Provision through the repository pin if CI has not used this package yet.
  // No Git command or fixture is run by this package-version query.
  const install = spawnSync(join(source, "bin/lefthook"), ["version"], {
    cwd: g.root,
    env: { ...g.env, HERMIT_STATE_DIR: cache },
    encoding: "utf8",
    timeout: 60000,
  });
  assert.equal(install.status, 0, install.stderr);
  assert.equal(install.stdout.trim(), pin.replace("lefthook-", ""));
  return realpathSync(join(cache, "pkg", pin, "lefthook"));
}

function dispatch(t, cwdKind, changed, fail = "", mutant = false) {
  const g = graph(t);
  const lefthook = pinnedLefthook(g);
  mkdirSync(join(g.main, "scripts"));
  writeFileSync(
    join(g.main, "scripts/hook-lane-wrapper.sh"),
    readFileSync(wrapper),
    { mode: 0o755 },
  );
  writeFileSync(
    join(g.main, "scripts/check-branch-skew.sh"),
    readFileSync(join(source, "scripts/check-branch-skew.sh")),
    { mode: 0o755 },
  );
  writeFileSync(
    join(g.main, "scripts/check-push-head-scope.sh"),
    readFileSync(join(source, "scripts/check-push-head-scope.sh")),
    { mode: 0o755 },
  );
  writeFileSync(
    join(g.main, "lefthook.yml"),
    mutant ? yaml.replaceAll("./scripts/hook-lane-wrapper.sh ", "") : yaml,
  );
  g.git(g.main, "add", ".");
  g.git(g.main, "commit", "-sm", "candidate wiring");
  g.git(g.linked, "merge", "--ff-only", "main");
  const remote = g.path("remote.git");
  g.git(g.main, "init", "--bare", remote);
  g.git(g.main, "remote", "add", "origin", remote);
  g.git(g.main, "push", "origin", "main", "main:candidate");
  g.git(
    g.main,
    "symbolic-ref",
    "refs/remotes/origin/HEAD",
    "refs/remotes/origin/main",
  );
  g.git(g.main, "branch", "--set-upstream-to=origin/candidate", "main");
  g.git(g.linked, "branch", "--set-upstream-to=origin/candidate", "linked");
  g.git(g.main, "config", "push.default", "upstream");
  const cwd = cwdKind === "main" ? g.main : g.linked;
  assert.equal(
    g.git(cwd, "rev-parse", "@{push}"),
    g.git(cwd, "rev-parse", "HEAD"),
  );
  const bin = g.path("lane stubs");
  mkdirSync(bin);
  const log = g.path("lane log");
  // Replace only costly executables. Actual YAML selection, wrapper, generated
  // hook and branch-skew script are unchanged, and the stub still runs real Git.
  const stub = `#!/bin/sh\nset -eu\nfor task in "$@"; do\n printf '%s\\n' "$task" >> "$FIXTURE_LOG"\n nested=$(mktemp -d "$FIXTURE_ROOT/nested.XXXXXX")\n "$FIXTURE_LANE" "$nested" > /dev/null\n [ "$task" != "$FAIL_LANE" ] || exit 37\ndone\n`;
  writeFileSync(join(bin, "just"), stub, { mode: 0o755 });
  writeFileSync(
    join(bin, "node"),
    `#!/bin/sh\nexec "$(dirname "$0")/just" hook-isolation-test\n`,
    { mode: 0o755 },
  );
  const env = {
    PATH: `${bin}:${g.env.PATH}`,
    LEFTHOOK_BIN: lefthook,
    FIXTURE_ROOT: g.root,
    FIXTURE_LOG: log,
    FIXTURE_LANE: g.lane,
    FAIL_LANE: fail,
  };
  const dest = g.path(
    cwdKind === "main" ? "scratch main" : "linked wt",
    changed,
  );
  mkdirSync(dirname(dest), { recursive: true });
  writeFileSync(
    dest,
    `${existsSync(dest) ? readFileSync(dest, "utf8") : ""}\n# fixture change\n`,
  );
  g.git(cwd, "add", changed);
  // Seed history before installing hooks: this regression targets pre-push.
  g.ok(cwd, "git", ["commit", "-sm", "changed path"], env);
  assert.match(
    g.git(cwd, "log", "-1", "--format=%B"),
    /Signed-off-by: Fixture/,
  );
  g.ok(cwd, lefthook, ["install", "--force"], env);
  writeFileSync(log, "");
  const before = g.snapshot();
  const branch = g.git(cwd, "branch", "--show-current");
  const result = g.run(
    cwd,
    "git",
    ["push", "origin", `${branch}:candidate`],
    env,
  );
  const selected = readFileSync(log, "utf8")
    .trim()
    .split("\n")
    .filter(Boolean)
    .sort();
  if (mutant) {
    assert.notEqual(
      g.snapshot().config,
      before.config,
      "bypassed wrapper must expose scratch corruption",
    );
    return;
  }
  const after = g.snapshot();
  assert.equal(after.config, before.config);
  assert.deepEqual(after.files, before.files);
  assert.equal(
    after.refs
      .split("\n")
      .filter((l) => !l.startsWith("refs/remotes/"))
      .join("\n"),
    before.refs
      .split("\n")
      .filter((l) => !l.startsWith("refs/remotes/"))
      .join("\n"),
  );
  assert.equal(
    result.status === 0,
    fail === "",
    `${result.stdout}\n${result.stderr}`,
  );
  const remoteRef = g.run(g.root, "git", [
    "--git-dir",
    remote,
    "rev-parse",
    "--verify",
    "refs/heads/candidate",
  ]);
  if (fail)
    assert.equal(
      remoteRef.stdout.trim(),
      g.git(cwd, "rev-parse", "HEAD^"),
      "failing lane must reject push",
    );
  else assert.equal(remoteRef.stdout.trim(), g.git(cwd, "rev-parse", "HEAD"));
  return selected;
}

test("pinned Lefthook dispatch preserves path selection and rejects failing pushes", async (t) => {
  const desktop = ["desktop-check", "desktop-test", "desktop-typecheck"];
  const native = ["desktop-tauri-clippy", "desktop-tauri-test"];
  for (const kind of ["main", "linked"]) {
    for (const [path, expected] of [
      ["desktop/src/fixture.ts", desktop],
      ["desktop/src-tauri/fixture.rs", native],
      ["crates/fixture.rs", [...native, "test-unit"]],
      ["mobile/fixture.dart", ["mobile-test"]],
      ["scripts/hook-lane-wrapper.sh", ["hook-isolation-test"]],
      ["lefthook.yml", ["hook-isolation-test"]],
      ["unrelated.md", []],
    ])
      await t.test(`${kind}: ${path}`, (t) =>
        assert.deepEqual(dispatch(t, kind, path), [...expected].sort()),
      );
  }
  await t.test("clippy failure stops tests and rejects linked push", (t) =>
    assert.deepEqual(
      dispatch(
        t,
        "linked",
        "desktop/src-tauri/fixture.rs",
        "desktop-tauri-clippy",
      ),
      ["desktop-tauri-clippy"],
    ),
  );
});

test("negative control reproduces exact linked GIT_DIR trigger only in scratch", (t) => {
  const g = graph(t);
  const nested = g.path("unprotected nested");
  mkdirSync(nested);
  const before = g.snapshot();
  g.ok(g.linked, "git", ["-C", nested, "init", "-b", "main"], {
    GIT_DIR: g.linkedDir,
  });
  assert.notEqual(readFileSync(join(g.common, "config"), "hex"), before.config);
  assert.equal(g.git(g.main, "config", "--local", "core.bare"), "true");
});

test("mutation bypassing production YAML boundary exposes scratch corruption", (t) => {
  dispatch(t, "linked", "desktop/src-tauri/fixture.rs", "", true);
});

function scopeFixture(t, stripSelectors = false) {
  const g = graph(t);
  const lefthook = pinnedLefthook(g);
  mkdirSync(join(g.main, "scripts"));
  for (const script of [
    "hook-lane-wrapper.sh",
    "check-branch-skew.sh",
    "check-push-head-scope.sh",
  ])
    writeFileSync(
      join(g.main, "scripts", script),
      readFileSync(join(source, "scripts", script)),
      { mode: 0o755 },
    );
  writeFileSync(
    join(g.main, "lefthook.yml"),
    stripSelectors
      ? yaml.replaceAll(
          "      files: git diff --name-only origin/main...HEAD\n",
          "",
        )
      : yaml,
  );
  const change = (cwd, file, content) => {
    mkdirSync(dirname(join(cwd, file)), { recursive: true });
    writeFileSync(join(cwd, file), content);
    g.git(cwd, "add", file);
    g.git(cwd, "commit", "-sm", `change ${JSON.stringify(file)}`);
  };
  g.git(g.main, "add", ".");
  g.git(g.main, "commit", "-sm", "production hook configuration");
  g.git(g.linked, "merge", "--ff-only", "main");
  const remote = g.path("scope remote.git");
  g.git(g.root, "init", "--bare", remote);
  g.git(g.main, "remote", "add", "origin", remote);
  g.git(g.main, "push", "origin", "main");
  g.git(
    g.main,
    "symbolic-ref",
    "refs/remotes/origin/HEAD",
    "refs/remotes/origin/main",
  );
  change(g.linked, "docs/scope.md", "branch docs");
  g.git(g.linked, "push", "-u", "origin", "linked");
  for (const file of [
    "crates/scope.rs",
    "desktop/src-tauri/scope.rs",
    "mobile/scope.dart",
  ])
    change(g.main, file, "incoming main");
  g.git(g.main, "push", "origin", "main");
  g.git(g.linked, "merge", "--no-edit", "main");
  g.git(g.linked, "fsck", "--full");
  const bin = g.path("scope stubs");
  const log = g.path("scope lanes");
  mkdirSync(bin);
  writeFileSync(
    join(bin, "just"),
    '#!/bin/sh\nprintf "%s\\n" "$@" >> "$FIXTURE_LOG"\n',
    { mode: 0o755 },
  );
  writeFileSync(
    join(bin, "node"),
    '#!/bin/sh\nprintf "hook-isolation-test\\n" >> "$FIXTURE_LOG"\n',
    { mode: 0o755 },
  );
  const env = {
    PATH: `${bin}:${g.env.PATH}`,
    LEFTHOOK_BIN: lefthook,
    FIXTURE_LOG: log,
  };
  // This fixture exercises pre-push; other generated hooks are outside its scope.
  g.ok(g.linked, lefthook, ["install", "pre-push", "--force"], env);
  // Commits after installation need the same cheap fixture executables too.
  const edit = (file, content) => {
    mkdirSync(dirname(join(g.linked, file)), { recursive: true });
    writeFileSync(join(g.linked, file), content);
    g.ok(g.linked, "git", ["add", file], env);
    g.ok(
      g.linked,
      "git",
      ["commit", "-sm", `change ${JSON.stringify(file)}`],
      env,
    );
  };
  const push = (ref = "linked") => {
    writeFileSync(log, "");
    const before = g.snapshot();
    const result = g.run(g.linked, "git", ["push", "origin", ref], env);
    const after = g.snapshot();
    assert.equal(after.config, before.config);
    assert.deepEqual(after.files, before.files);
    assert.equal(
      after.refs
        .split("\n")
        .filter((line) => !line.startsWith("refs/remotes/"))
        .join("\n"),
      before.refs
        .split("\n")
        .filter((line) => !line.startsWith("refs/remotes/"))
        .join("\n"),
    );
    return {
      ...result,
      lanes: readFileSync(log, "utf8")
        .trim()
        .split("\n")
        .filter(Boolean)
        .sort(),
    };
  };
  return { g, remote, edit, push };
}

function assertDocsOnly(result) {
  assert.equal(result.status, 0, `${result.stdout}\n${result.stderr}`);
  assert.deepEqual(
    result.lanes,
    [],
    "docs-only merge refresh must select no expensive lane",
  );
}

test("merge-base selectors reject incoming-main lanes and preserve incremental positives", (t) => {
  const f = scopeFixture(t);
  assertDocsOnly(f.push());
  f.edit("mobile/scope.dart", "branch mobile");
  let result = f.push();
  assert.equal(result.status, 0, result.stderr);
  assert.deepEqual(result.lanes, ["mobile-test"]);
  f.edit("crates/scope.rs", "branch rust");
  result = f.push();
  assert.equal(result.status, 0, result.stderr);
  assert.deepEqual(result.lanes, [
    "desktop-tauri-clippy",
    "desktop-tauri-test",
    "test-unit",
  ]);
  for (const name of ["internal space.dart", "line\nbreak.dart"]) {
    f.edit(`mobile/${name}`, "branch mobile edge");
    result = f.push();
    assert.equal(result.status, 0, result.stderr);
    assert.deepEqual(result.lanes, ["mobile-test"]);
  }
  // Existing non-HEAD behavior remains: checks cover HEAD, warning names other ref.
  f.g.git(f.g.linked, "branch", "other");
  f.g.git(f.g.linked, "switch", "-c", "docs-only", "origin/main");
  f.edit("docs/scope.md", "docs only");
  result = f.push("other:other");
  assertDocsOnly(result);
  assert.match(
    result.stdout + result.stderr,
    /Pushing commits that are not the checked-out HEAD/,
  );
  assert.match(result.stdout + result.stderr, /refs\/heads\/other/);
  assert.equal(
    f.g.git(f.g.root, "--git-dir", f.remote, "rev-parse", "other"),
    f.g.git(f.g.linked, "rev-parse", "other"),
  );
  // Keep the bare source missing too: the parallel branch-skew fetch cannot repair it.
  const before = f.g.git(
    f.g.root,
    "--git-dir",
    f.remote,
    "rev-parse",
    "linked",
  );
  f.g.git(f.g.linked, "update-ref", "-d", "refs/remotes/origin/main");
  f.g.git(
    f.g.root,
    "--git-dir",
    f.remote,
    "update-ref",
    "-d",
    "refs/heads/main",
  );
  f.g.git(f.g.linked, "switch", "linked");
  f.edit("docs/scope.md", "missing base");
  result = f.push();
  assert.notEqual(result.status, 0);
  assert.deepEqual(result.lanes, []);
  assert.match(result.stdout + result.stderr, /origin\/main\.\.\.HEAD/);
  assert.equal(
    f.g.git(f.g.root, "--git-dir", f.remote, "rev-parse", "linked"),
    before,
  );
});

test("removing selectors falsifies the healthy docs-refresh assertion", (t) => {
  const f = scopeFixture(t, true);
  const result = f.push();
  assert.equal(result.status, 0, result.stderr);
  assert.throws(() => assertDocsOnly(result), /docs-only merge refresh/);
  assert.deepEqual(result.lanes, [
    "desktop-tauri-clippy",
    "desktop-tauri-test",
    "mobile-test",
    "test-unit",
  ]);
  f.edit("crates/scope.rs", "branch rust");
  const positive = f.push();
  assert.equal(positive.status, 0, positive.stderr);
  assert.deepEqual(positive.lanes, [
    "desktop-tauri-clippy",
    "desktop-tauri-test",
    "test-unit",
  ]);
});

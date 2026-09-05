import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";
import test from "node:test";

const patch = await readFile(
  new URL("../../../../../patches/virtua@0.49.3.patch", import.meta.url),
  "utf8",
);

test("reader wheel retires Virtua shift mode without publishing scroll end", () => {
  // Virtua 0.49.3 has no public transition for leaving SCROLL_BY_SHIFT.
  // Keep the CJS and ESM patch paths symmetric and deliberately narrower than
  // ACTION_SCROLL_END (2), which also idles direction, clears frozen range,
  // flushes pending jumps, and emits UPDATE_SCROLL_END_EVENT.
  const addedActionBodies = [
    ...patch.matchAll(/\+\s+case 9:\n((?:\+.*\n)+?)(?=\s*})/g),
  ].map(([, body]) =>
    body
      .split("\n")
      .map((line) => line.replace(/^\+\s*/, "").trim())
      .filter(Boolean),
  );
  assert.deepEqual(addedActionBodies, [
    ["I = 0, pendingShiftAck = false;"],
    ["w = 0, pendingShiftAck = false;"],
  ]);
  assert.match(
    patch,
    /if \(!e\.M\(\) \|\| t\.ctrlKey\) return;\n\+\s+e\.q\(9\);\n\+\s+if \(f\) return;/,
  );
  assert.match(
    patch,
    /if \(!e\.M\(\) \|\| t\.ctrlKey\) return;\n\+\s+e\.B\(9\);\n\+\s+if \(c\) return;/,
  );
  assert.doesNotMatch(
    patch,
    /\+\s+if \((?:f|c) \|\| !e\.M\(\) \|\| t\.ctrlKey\) return;/,
  );
  assert.doesNotMatch(patch, /\+\s+e\.(?:q|B)\(2\);/);
});

const runtimeUrl = new URL(import.meta.resolve("virtua"));
const formats = [
  {
    file: "index.js",
    start: "const u = null",
    end: "}, L = (e, t) => {",
    store: "E",
    scroller: "A",
    update: "B",
    attach: "N",
    flush: "q",
  },
  {
    file: "index.cjs",
    start: "const r = null",
    end: "}, O = (e, t) => {",
    store: "I",
    scroller: "M",
    update: "q",
    attach: "L",
    flush: "V",
  },
];

async function createScrollHarness(
  format,
  { viewport = 200, initial = 100, round = false, maximum } = {},
) {
  const source = await readFile(new URL(format.file, runtimeUrl), "utf8");
  const start = source.indexOf(format.start);
  const end = source.indexOf(format.end, start);
  assert.ok(
    start >= 0 && end > start,
    "installed Virtua core boundaries exist",
  );
  const timers = new Map();
  let timerId = 0;
  // Execute the shipped store and DOM scroller, excluding their React wrappers.
  const runtime = new Function(
    "navigator",
    "document",
    "setTimeout",
    "clearTimeout",
    `${source.slice(start, end + 1)};return {store:${format.store},scroller:${format.scroller}};`,
  )(
    { userAgent: "Linux", platform: "Linux", maxTouchPoints: 0 },
    { documentElement: { style: {} } },
    (callback) => {
      timers.set(++timerId, callback);
      return timerId;
    },
    (id) => timers.delete(id),
  );
  const store = runtime.store(10, Array(10).fill(100));
  const update = (action, payload) => store[format.update](action, payload);
  update(4, viewport);
  let offset = initial;
  const element = new EventTarget();
  element.style = {};
  Object.defineProperty(element, "scrollTop", {
    get: () => offset,
    set: (value) => {
      offset = Math.max(
        0,
        Math.min(
          maximum ?? store.h() - viewport,
          round ? Math.round(value) : value,
        ),
      );
    },
  });
  const scroller = runtime.scroller(store, false);
  scroller[format.attach]({}, element);
  const acknowledge = () => element.dispatchEvent(new Event("scroll"));
  acknowledge();
  let ends = 0;
  store.W(8, () => {
    ends += 1;
  });
  return {
    update,
    acknowledge,
    prepend: (prefix = [300, 300, 100]) =>
      update(5, [
        10 + prefix.length,
        true,
        [...prefix, ...Array(10).fill(100)],
      ]),
    flush: () => scroller[format.flush](),
    end: () => {
      const [id, callback] = timers.entries().next().value ?? [];
      assert.ok(callback, "a real scroll observer timeout is pending");
      timers.delete(id);
      callback();
    },
    wheel: (ctrlKey = false) => {
      const event = new Event("wheel");
      Object.assign(event, { deltaY: 100, ctrlKey });
      element.dispatchEvent(event);
    },
    get offset() {
      return offset;
    },
    get anchorTop() {
      return store.k(3) - offset;
    },
    get ends() {
      return ends;
    },
    close: () => scroller.v(),
  };
}

for (const format of formats) {
  test(`${format.file}: old scroll end cannot retire an unacknowledged prepend`, async () => {
    const h = await createScrollHarness(format);
    try {
      h.prepend();
      h.flush();
      h.flush(); // Empty layout flush is not a DOM acknowledgement.
      h.end();
      h.acknowledge();
      h.update(3, [
        [0, 104],
        [1, 104],
      ]);
      h.flush();
      assert.equal(h.offset, 408);
      h.update(3, [[2, 56]]);
      h.flush();
      assert.equal(h.offset, 364);
      assert.equal(h.anchorTop, -100);
      assert.equal(h.ends, 0);
      h.acknowledge();
      h.end();
      assert.equal(
        h.ends,
        1,
        "acknowledged corrections allow normal scroll end",
      );
    } finally {
      h.close();
    }
  });

  for (const intent of ["wheel", "manual", "append"]) {
    test(`${format.file}: ${intent} retires pending shift ownership`, async () => {
      const h = await createScrollHarness(format);
      try {
        h.prepend();
        h.flush();
        if (intent === "wheel") h.wheel();
        if (intent === "manual") h.update(7);
        if (intent === "append") h.update(5, [14, false, Array(14).fill(100)]);
        h.end();
        assert.equal(h.ends, 1);
      } finally {
        h.close();
      }
    });
  }

  test(`${format.file}: Ctrl+wheel preserves pending prepend acknowledgement`, async () => {
    const h = await createScrollHarness(format);
    try {
      h.prepend();
      h.flush();
      h.wheel(true);
      h.end();
      assert.equal(h.ends, 0);
      h.acknowledge();
      h.end();
      assert.equal(h.ends, 1);
    } finally {
      h.close();
    }
  });

  for (const [name, options, prefix] of [
    ["fitting", { viewport: 2000, initial: 0 }, undefined],
    ["exact fit", { viewport: 1700, initial: 0 }, undefined],
    ["clamped", { initial: 0, maximum: 0 }, undefined],
    ["rounded", { round: true }, [0.25]],
  ]) {
    test(`${format.file}: ${name} no-op shift cannot strand scrolling`, async () => {
      const h = await createScrollHarness(format, options);
      try {
        const before = h.offset;
        h.prepend(prefix);
        h.flush();
        h.end();
        assert.equal(h.offset, before);
        assert.equal(h.ends, 1);
      } finally {
        h.close();
      }
    });
  }
}

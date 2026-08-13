(() => {
  if (window.__CREW_AGENT_BRIDGE__) return;
  const state = {
    console: [],
    refEls: new Map(),
  };

  function pushConsole(kind, text, extra) {
    state.console.push(
      Object.assign({ t_ms: Date.now(), kind, text }, extra || {}),
    );
    if (state.console.length > 200) state.console.shift();
  }

  const orig = {
    log: console.log,
    warn: console.warn,
    error: console.error,
  };
  console.log = (...args) => {
    pushConsole("log", args.join(" "));
    return orig.log.apply(console, args);
  };
  console.warn = (...args) => {
    pushConsole("warn", args.join(" "));
    return orig.warn.apply(console, args);
  };
  console.error = (...args) => {
    pushConsole("error", args.join(" "));
    return orig.error.apply(console, args);
  };
  window.addEventListener("error", (ev) => {
    pushConsole("pageerror", String(ev.message || ev.error || "error"));
  });

  const origFetch = window.fetch;
  if (origFetch) {
    window.fetch = function (input, init) {
      const started = Date.now();
      const method = init?.method || "GET";
      const url = typeof input === "string" ? input : input?.url || "";
      return origFetch.call(this, input, init).then((res) => {
        const size = Number(res.headers.get("content-length") || 0);
        pushConsole("network", `${method} ${url} ${res.status}`, {
          method,
          url,
          status: res.status,
          duration_ms: Date.now() - started,
          size,
        });
        return res;
      });
    };
  }
  const XO = window.XMLHttpRequest;
  if (XO) {
    const open = XO.prototype.open;
    const send = XO.prototype.send;
    XO.prototype.open = function (method, url, ...rest) {
      this.__crew = { method, url, started: 0 };
      return open.call(this, method, url, ...rest);
    };
    XO.prototype.send = function (...args) {
      if (this.__crew) this.__crew.started = Date.now();
      this.addEventListener("loadend", () => {
        const meta = this.__crew || {};
        pushConsole("network", `${meta.method} ${meta.url} ${this.status}`, {
          method: meta.method,
          url: meta.url,
          status: this.status,
          duration_ms: meta.started ? Date.now() - meta.started : 0,
          size: 0,
        });
      });
      return send.apply(this, args);
    };
  }

  function roleOf(el) {
    return (
      el.getAttribute("role") ||
      {
        A: "link",
        BUTTON: "button",
        INPUT: el.type === "checkbox" ? "checkbox" : "textbox",
        SELECT: "combobox",
        TEXTAREA: "textbox",
        SUMMARY: "button",
      }[el.tagName] ||
      "generic"
    );
  }
  function nameOf(el) {
    return (
      el.getAttribute("aria-label") ||
      el.getAttribute("alt") ||
      el.getAttribute("placeholder") ||
      (el.innerText || "").trim().slice(0, 80)
    );
  }
  function interactive(el) {
    const tag = el.tagName;
    if (["A", "BUTTON", "INPUT", "SELECT", "TEXTAREA", "SUMMARY"].includes(tag))
      return true;
    const role = el.getAttribute("role");
    return (
      !!el.onclick ||
      el.isContentEditable ||
      [
        "button",
        "link",
        "textbox",
        "checkbox",
        "radio",
        "combobox",
        "menuitem",
        "tab",
        "switch",
      ].includes(role)
    );
  }
  function visible(el) {
    const s = window.getComputedStyle(el);
    if (s.display === "none" || s.visibility === "hidden" || s.opacity === "0")
      return false;
    const r = el.getBoundingClientRect();
    return r.width > 0 && r.height > 0;
  }

  function visit(el, filterAll, accEls) {
    if (el?.nodeType !== 1) return [];
    const isInt = interactive(el);
    if (!filterAll && !isInt) {
      const out = [];
      for (const child of el.children)
        out.push(...visit(child, filterAll, accEls));
      return out;
    }
    const r = el.getBoundingClientRect();
    const node = {
      ref: "",
      role: roleOf(el),
      name: nameOf(el),
      value: "value" in el ? String(el.value || "") : null,
      actionable: isInt && visible(el) && !el.disabled,
      bounds: { x: r.x, y: r.y, w: r.width, h: r.height },
      children: [],
    };
    accEls.push(el);
    for (const child of el.children) {
      node.children.push(...visit(child, filterAll, accEls));
    }
    return [node];
  }

  function mint(nodes, n, els, i) {
    for (const node of nodes) {
      node.ref = `e${n.value}`;
      if (els[i.value]) state.refEls.set(node.ref, els[i.value]);
      n.value += 1;
      i.value += 1;
      mint(node.children || [], n, els, i);
    }
  }

  async function waitActionable(el, ms) {
    const start = Date.now();
    while (Date.now() - start < ms) {
      if (visible(el) && !el.disabled) return true;
      await new Promise((r) => setTimeout(r, 50));
    }
    return false;
  }

  function reply(id, payload) {
    const url = window.__CREW_BRIDGE_REPLY_URL;
    const nonce = window.__CREW_BRIDGE_NONCE;
    if (!url || !nonce) return;
    fetch(url, {
      method: "POST",
      mode: "no-cors",
      headers: { "content-type": "text/plain" },
      body: JSON.stringify({ nonce, id, payload }),
    });
  }

  window.__CREW_AGENT_BRIDGE__ = {
    snapshot(filter) {
      const els = [];
      const nodes = document.body
        ? visit(document.body, filter === "all", els)
        : [];
      mint(nodes, { value: 1 }, els, { value: 0 });
      return {
        url: location.href,
        title: document.title,
        origin: location.origin,
        nodes,
      };
    },
    async clickAt(x, y) {
      const el = document.elementFromPoint(x, y);
      if (!el) return { ok: false, reason: "missing" };
      if (!(await waitActionable(el, 5000)))
        return { ok: false, reason: "not_actionable" };
      el.click();
      return { ok: true };
    },
    async clickRef(ref) {
      const el = state.refEls.get(ref);
      if (!el) return { ok: false, reason: "missing" };
      if (!(await waitActionable(el, 5000)))
        return { ok: false, reason: "not_actionable" };
      el.click();
      return { ok: true };
    },
    async typeAt(x, y, text, submit) {
      const el = document.elementFromPoint(x, y);
      if (!el) return { ok: false, reason: "missing" };
      if (!(await waitActionable(el, 5000)))
        return { ok: false, reason: "not_actionable" };
      el.focus();
      if ("value" in el) el.value = text;
      el.dispatchEvent(new Event("input", { bubbles: true }));
      if (submit) {
        el.dispatchEvent(
          new KeyboardEvent("keydown", { key: "Enter", bubbles: true }),
        );
      }
      return { ok: true };
    },
    async typeRef(ref, text, submit) {
      const el = state.refEls.get(ref);
      if (!el) return { ok: false, reason: "missing" };
      if (!(await waitActionable(el, 5000)))
        return { ok: false, reason: "not_actionable" };
      el.focus();
      if ("value" in el) el.value = text;
      el.dispatchEvent(new Event("input", { bubbles: true }));
      if (submit) {
        el.dispatchEvent(
          new KeyboardEvent("keydown", { key: "Enter", bubbles: true }),
        );
      }
      return { ok: true };
    },
    scroll(ref, direction, amount) {
      const dx =
        direction === "left"
          ? -(amount || 400)
          : direction === "right"
            ? amount || 400
            : 0;
      const dy =
        direction === "up"
          ? -(amount || 400)
          : direction === "down"
            ? amount || 400
            : 0;
      const el = ref ? state.refEls.get(ref) : null;
      if (el) el.scrollBy(dx, dy);
      else window.scrollBy(dx, dy);
      return { ok: true };
    },
    evaluate(js) {
      // D-059: full browser_evaluate — arbitrary JS in the subject origin.
      // biome-ignore lint/security/noGlobalEval: this is the evaluate tool
      return window.eval(js);
    },
    console(since) {
      return state.console.filter((e) => !since || e.t_ms >= since);
    },
    reply,
    replySnapshot(id, filter) {
      reply(id, this.snapshot(filter));
    },
    replyConsole(id, since) {
      reply(id, { entries: this.console(since) });
    },
    replyEvaluate(id, js) {
      try {
        reply(id, { result: this.evaluate(js) });
      } catch (err) {
        reply(id, { error: String(err) });
      }
    },
    replyClickRef(id, ref) {
      this.clickRef(ref).then((payload) => reply(id, payload));
    },
    replyTypeRef(id, ref, text, submit) {
      this.typeRef(ref, text, submit).then((payload) => reply(id, payload));
    },
    replyClickAt(id, x, y) {
      this.clickAt(x, y).then((payload) => reply(id, payload));
    },
  };
})();

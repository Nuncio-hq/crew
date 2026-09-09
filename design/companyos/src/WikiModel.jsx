import { createContext, useContext, useEffect, useRef, useState } from "react";
import { sampleWikiAnswer, wikiPages, wikiRevision } from "./wiki-content";

const WikiContext = createContext(null);
export const wikiKey = (project, repository) =>
  `${project}::${repository || "unlinked"}`;
export function blankWiki(key) {
  const seeded = key === "NuncioCrew::crew";
  return {
    pages: seeded ? wikiPages : [],
    page: seeded ? "workspaces" : "overview",
    view: "read",
    source: null,
    health: seeded ? "ready" : "empty",
    history: [],
    activeQuestion: null,
    questionDraft: "",
    taskDraft: null,
    askAgent: "Codex",
    askUnavailable: false,
    askFail: false,
    settings: { runtime: seeded ? "Codex" : "", model: "", profile: "" },
    updated: "12 minutes ago",
    revision: wikiRevision,
    scroll: {},
    job: null,
  };
}
export const generationSteps = [
  "Read source snapshot",
  "Plan the pages",
  "Write changed pages",
  "Check source references",
];

function genericPages(meta) {
  return [
    {
      id: "overview",
      title: `${meta.project} overview`,
      section: "Getting oriented",
      description: "A sample first page for this connected workspace.",
      keywords: "project overview workspace",
      sections: [
        {
          title: "Connected workspace",
          text: `${meta.name} is linked to ${meta.project}. This preview uses the selected sample ${meta.kind === "git" ? "repository" : "folder"}; it has not read your local files.`,
        },
        {
          title: "Ready for real indexing",
          text: "A production update would read this workspace, write its pages and verify source references. This sample page lets you review the empty-to-generated flow without running an agent.",
        },
      ],
      related: [],
    },
  ];
}

export function WikiProvider({ children }) {
  const [records, setRecords] = useState({
    "NuncioCrew::crew": blankWiki("NuncioCrew::crew"),
  });
  const [repositories, setRepositories] = useState({});
  const timers = useRef(new Map());
  const serial = useRef(0);
  function patch(key, change) {
    setRecords((current) => {
      const previous = current[key] || blankWiki(key);
      return {
        ...current,
        [key]:
          typeof change === "function"
            ? change(previous)
            : { ...previous, ...change },
      };
    });
  }
  function later(id, callback, ms) {
    const timer = setTimeout(() => {
      timers.current.delete(id);
      callback();
    }, ms);
    timers.current.set(id, timer);
  }
  function clearAll() {
    for (const timer of timers.current.values()) clearTimeout(timer);
    timers.current.clear();
  }
  useEffect(() => clearAll, []);
  function generate(key, meta, settings) {
    const run = ++serial.current;
    patch(key, (r) => ({
      ...r,
      settings,
      health: "updating",
      job: { run, step: 0 },
    }));
    for (let step = 1; step <= 4; step++) {
      later(
        `update-${run}-${step}`,
        () =>
          patch(key, (r) => {
            if (r.job?.run !== run) return r;
            if (step < 4) return { ...r, job: { run, step } };
            const pages =
              key === "NuncioCrew::crew" ? wikiPages : genericPages(meta);
            return {
              ...r,
              pages,
              page: pages.some((p) => p.id === r.page) ? r.page : pages[0].id,
              health: "ready",
              updated: "Just now",
              job: null,
            };
          }),
        step * 1100,
      );
    }
  }
  function ask(key, prompt, agent) {
    const run = ++serial.current,
      id = `question-${run}`;
    patch(key, (r) => {
      const previous = r.history.find((q) => q.id === r.activeQuestion);
      const answer = sampleWikiAnswer(
        prompt,
        r.pages,
        previous?.answer?.pageId || r.page,
      );
      const question = {
        id,
        run,
        prompt,
        agent,
        state: "reading",
        answer,
        revision: r.revision,
        pageId: r.page,
      };
      return {
        ...r,
        view: "ask",
        source: null,
        questionDraft: "",
        activeQuestion: id,
        history: [...r.history, question].slice(-30),
      };
    });
    later(
      `ask-${run}`,
      () =>
        patch(key, (r) => ({
          ...r,
          history: r.history.map((q) =>
            q.id === id && q.state === "reading"
              ? { ...q, state: r.askFail ? "failed" : "answered" }
              : q,
          ),
          askFail: false,
        })),
      1700,
    );
  }
  const value = {
    get: (key) => records[key] || blankWiki(key),
    patch,
    repositories,
    selectRepository: (project, id) =>
      setRepositories((r) => ({ ...r, [project]: id })),
    generate,
    ask,
    cancelUpdate: (key) =>
      patch(key, (r) => ({
        ...r,
        health: r.pages.length ? "stale" : "empty",
        job: null,
      })),
    cancelAsk: (key) =>
      patch(key, (r) => ({
        ...r,
        history: r.history.map((q) =>
          q.id === r.activeQuestion ? { ...q, state: "stopped" } : q,
        ),
      })),
    reset: () => {
      clearAll();
      setRecords({ "NuncioCrew::crew": blankWiki("NuncioCrew::crew") });
      setRepositories({});
    },
  };
  return <WikiContext.Provider value={value}>{children}</WikiContext.Provider>;
}
export const useWiki = () => useContext(WikiContext);

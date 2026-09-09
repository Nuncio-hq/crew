import { RecapProvider, useRecap } from "./RecapSettings";
import { sampleRecipients } from "./contact-routing";
import { AgentDirectoryProvider, useAgentDirectory } from "./AgentDirectory";
import { ChannelRolesProvider, useChannelRoles } from "./ChannelRoles";
import { useState, useRef, useEffect, useMemo } from "react";
import { FilmControls, FilmCursor } from "./FilmPlayer";
import { filmFrameAt, FILM_DURATION } from "./film";
import { scenarios } from "./blueprint";
import { ReviewBar, Guide } from "./ReviewBar";
import { Sidebar } from "./Sidebar";
import { Conversation } from "./Conversation";
import { WorkspacePages } from "./WorkspacePages";
import { activityAt } from "./activity-model";
import { sampleThreads } from "./thread-model";
import { Tools } from "./Tools";
import { Icon, IconButton } from "./ui";
const initial =
  scenarios.find(
    (s) => s.id === new URLSearchParams(location.search).get("state"),
  ) || scenarios.find((s) => s.id === "thread-working");
const initialThread =
  sampleThreads.find(
    (t) => t.id === new URLSearchParams(location.search).get("thread"),
  ) ||
  sampleThreads.find(
    (t) =>
      t.id ===
      (initial.id === "blocked"
        ? "ci"
        : initial.id === "discussion"
          ? "discussion"
          : initial.id === "completed"
            ? "archive"
            : initial.id === "needs-you"
              ? "release"
              : initial.id === "done"
                ? "handoff"
                : initial.id === "offline"
                  ? "remote"
                  : "workspace"),
  );
function Workspace() {
  const { agents, allAgents, reset: resetAgents } = useAgentDirectory();
  const { reset: resetRoles } = useChannelRoles();
  const { reset: resetRecap } = useRecap();
  const [filmActive, setFilmActive] = useState(
    new URLSearchParams(location.search).get("film") !== "off",
  );
  const [filmTime, setFilmTime] = useState(0);
  const [activityTime, setActivityTime] = useState(0);
  const [activityEpoch, setActivityEpoch] = useState(0);
  const [filmPlaying, setFilmPlaying] = useState(true);
  const [filmSpeed, setFilmSpeed] = useState(1);
  const filmFrame = useMemo(
    () => (filmActive ? filmFrameAt(filmTime) : null),
    [filmActive, filmTime],
  );
  function startFilm() {
    history.replaceState(null, "", "?film=on");
    setFilmTime(0);
    setFilmActive(true);
    setFilmPlaying(true);
  }

  const [explorationEvidence, setExplorationEvidence] = useState(null);
  const [initialActivity, setInitialActivity] = useState(false);
  const [selectedPR, setSelectedPR] = useState(null);
  const [threadList, setThreadList] = useState(sampleThreads);
  const [selectedThread, setSelectedThread] = useState(initialThread.id);
  const currentThread = threadList.find((t) => t.id === selectedThread);
  const removedIds = allAgents
    .filter((a) => a.deleted)
    .map((a) => a.id)
    .join(",");
  useEffect(() => {
    const removed = new Set(
      allAgents.filter((a) => a.deleted).map((a) => a.id),
    );
    if (threadList.some((t) => removed.has(t.owner) && t.status === "working"))
      setThreadList((current) =>
        current.map((t) =>
          removed.has(t.owner) && t.status === "working"
            ? {
                ...t,
                status: "stopped",
                next: "Owner removed. Choose a new owner before resuming work.",
              }
            : t,
        ),
      );
  }, [removedIds, threadList]);
  function updateThread(patch) {
    if (["stopped", "offline"].includes(patch.status)) {
      patch = {
        ...patch,
        activity:
          currentThread.activity || activityAt(currentThread, activityTime),
      };
    }
    setThreadList((ts) =>
      ts.map((t) => (t.id === selectedThread ? { ...t, ...patch } : t)),
    );
    if (patch.status) setStatus(patch.status);
  }
  function openThread(id, tab = "plan", prId = null) {
    const t = threadList.find((t) => t.id === id);
    setScenario(
      {
        "needs-you": "needs-you",
        done: "done",
        completed: "completed",
        offline: "offline",
        blocked: "blocked",
        discussion: "discussion",
      }[t.status] || "thread-working",
    );
    setScreen("thread");
    setTools(true);
    setProject(t.project || "NuncioCrew");
    setChannel(t.channel || "product");
    setSearch(false);
    setSelectedThread(id);
    setStatus(t.status);
    setToolTab(tab === "activity" ? "plan" : tab);
    setInitialActivity(tab === "activity");
    setSelectedPR(prId);
    history.replaceState(
      null,
      "",
      `?film=off&state=thread-working&thread=${id}`,
    );
  }
  const [scenario, setScenario] = useState(initial.id),
    [screen, setScreen] = useState(initial.screen),
    [status, setStatus] = useState(
      initial.screen === "thread" ? initialThread.status : initial.status,
    ),
    [tools, setTools] = useState(initial.tools),
    [toolTab, setToolTab] = useState("file"),
    [guide, setGuide] = useState(false),
    [sidebar, setSidebar] = useState(true),
    [agent, setAgent] = useState("Hermes"),
    [project, setProject] = useState(
      initial.screen === "empty"
        ? "HeardBack"
        : initialThread.project || "NuncioCrew",
    ),
    [channel, setChannel] = useState(
      initial.screen === "empty"
        ? "general"
        : initialThread.channel || "product",
    ),
    [messages, setMessages] = useState({}),
    [drafts, setDrafts] = useState({}),
    [search, setSearch] = useState(false),
    [query, setQuery] = useState(""),
    [toast, setToast] = useState(""),
    [toolWidth, setToolWidth] = useState(46);
  useEffect(() => {
    if (filmActive) return;
    const interval = setInterval(
      () =>
        setActivityTime((t) => {
          if (t >= 24) {
            clearInterval(interval);
            return t;
          }
          return Math.min(24, t + 0.2);
        }),
      200,
    );
    return () => clearInterval(interval);
  }, [filmActive, activityEpoch]);
  const { contactPoint } = useChannelRoles(project, channel);
  const scrollPositions = useRef({}),
    timer = useRef(null),
    appRef = useRef(null),
    toastTimer = useRef(null);
  const key =
    screen === "dm"
      ? `dm:${agent}`
      : screen === "thread"
        ? `thread:${selectedThread}`
        : `${screen}:${project}:${channel}`;
  function notice(t) {
    setToast(t);
    clearTimeout(toastTimer.current);
    toastTimer.current = setTimeout(() => setToast(""), 4000);
  }
  function idToThread(id) {
    return id === "blocked"
      ? "ci"
      : id === "discussion"
        ? "discussion"
        : id === "completed"
          ? "archive"
          : id === "needs-you"
            ? "release"
            : id === "done"
              ? "handoff"
              : id === "offline"
                ? "remote"
                : "workspace";
  }
  function load(id) {
    setInitialActivity(false);
    setActivityTime(0);
    setActivityEpoch((e) => e + 1);
    const s = scenarios.find((x) => x.id === id);
    if (!s) return;
    clearTimeout(timer.current);
    setScenario(id);
    setScreen(s.screen);
    if (s.screen === "thread" || s.screen === "channel") {
      setProject("NuncioCrew");
      setChannel("product");
    }
    if (s.screen === "empty") {
      setProject("HeardBack");
      setChannel("general");
    }
    if (s.screen === "thread") {
      const threadId = idToThread(id);
      setSelectedThread(threadId);
      setThreadList((ts) =>
        ts.map((t) =>
          t.id === threadId
            ? {
                ...sampleThreads.find((sample) => sample.id === threadId),
                status: s.status,
              }
            : t,
        ),
      );
    }
    setStatus(s.status);
    setTools(s.tools);
    setSearch(false);
    setQuery("");
    history.replaceState(null, "", `?film=off&state=${id}`);
  }
  function go(s, context) {
    const map = {
      thread: "thread-working",
      channel: "channel",
      dm: "dm",
      empty: "empty",
      inbox: "inbox",
      wiki: "wiki",
      agents: "agents",
      workflows: "workflows",
      settings: "settings",
    };
    if (s === "thread") {
      openThread("workspace");
      return;
    }
    if (map[s]) {
      const returnContext =
        s === "channel" && !context ? { project, channel } : context;
      load(map[s]);
      context = returnContext;
      if (context) {
        setProject(context.project);
        setChannel(context.channel);
      }
    } else {
      setScreen(s);
      setTools(false);
      setSearch(false);
    }
  }
  function reset() {
    resetRecap();
    resetRoles();
    resetAgents();
    setActivityTime(0);
    setActivityEpoch((e) => e + 1);
    clearTimeout(timer.current);
    setMessages({});
    setDrafts({});
    scrollPositions.current = {};
    setToolTab("file");
    setToolWidth(46);
    setAgent("Hermes");
    setThreadList(sampleThreads);
    load("thread-working");
    notice("Prototype reset");
  }
  function send(text) {
    const route =
      screen === "dm"
        ? { recipients: [], unavailable: [] }
        : sampleRecipients({ text, agents, project, channel, contactPoint });
    if (route.unavailable.length || route.missingContact) {
      notice(
        route.missingContact
          ? "Contact point is no longer a channel member. Update Assign roles."
          : `${route.unavailable.join(", ")} is unavailable. No sample reply was generated.`,
      );
    }
    setMessages((m) => ({
      ...m,
      [key]: [
        ...(m[key] || []),
        { author: "Oscar", text },
        ...route.recipients.map((recipient) => ({
          author: recipient,
          text: "Sample reply: I have your message. What outcome would you like us to agree on first?",
        })),
      ],
    }));
    if (screen === "dm") {
      const recipient = agent,
        capturedKey = key;
      clearTimeout(timer.current);
      timer.current = setTimeout(
        () =>
          setMessages((m) => ({
            ...m,
            [capturedKey]: [
              ...(m[capturedKey] || []),
              {
                author: recipient,
                text: "I have your message. What outcome would you like us to agree on first?",
              },
            ],
          })),
        900,
      );
    }
  }
  useEffect(
    () => () => {
      clearTimeout(timer.current);
      clearTimeout(toastTimer.current);
    },
    [],
  );
  useEffect(() => {
    function handle(e) {
      if ((e.metaKey || e.ctrlKey) && e.key === "k") {
        e.preventDefault();
        setSearch((x) => !x);
      }
      if (e.key === "Escape") {
        if (search) setSearch(false);
        else if (guide) setGuide(false);
        else if (screen === "thread") go("channel");
      }
    }
    window.addEventListener("keydown", handle);
    return () => window.removeEventListener("keydown", handle);
  }, [screen, search, guide]);
  function resize(e) {
    const start = e.clientX;
    const width = appRef.current?.clientWidth || 1000;
    const current = toolWidth;
    e.currentTarget.setPointerCapture(e.pointerId);
    function move(v) {
      setToolWidth(
        Math.min(
          60,
          Math.max(30, current + ((start - v.clientX) / width) * 100),
        ),
      );
    }
    function end() {
      window.removeEventListener("pointermove", move);
      window.removeEventListener("pointerup", end);
    }
    window.addEventListener("pointermove", move);
    window.addEventListener("pointerup", end, { once: true });
  }
  useEffect(() => {
    if (!filmActive || !filmPlaying) return;
    let last = performance.now();
    const interval = setInterval(() => {
      const now = performance.now(),
        delta = Math.min(500, now - last) / 1000;
      last = now;
      setFilmTime((t) => Math.min(FILM_DURATION, t + delta * filmSpeed));
    }, 100);
    return () => clearInterval(interval);
  }, [filmActive, filmPlaying, filmSpeed]);
  useEffect(() => {
    if (filmTime >= FILM_DURATION) setFilmPlaying(false);
  }, [filmTime]);
  useEffect(() => {
    if (!filmFrame) return;
    setThreadList(filmFrame.threads);
    setMessages(filmFrame.messages);
    setScreen(filmFrame.screen);
    setProject(filmFrame.project);
    setChannel(filmFrame.channel);
    setSelectedThread(filmFrame.thread || "workspace");
    setTools(!!filmFrame.tool);
    setToolTab(filmFrame.tool || "plan");
    setSelectedPR(filmFrame.thread === "tracking" ? "tracking" : null);
    const scope =
      filmFrame.screen === "thread"
        ? `thread:${filmFrame.thread}`
        : `channel:${filmFrame.project}:${filmFrame.channel}`;
    setDrafts({ [scope]: filmFrame.draft });
  }, [filmFrame]);
  const conversation = ["thread", "channel", "dm", "empty"].includes(screen);
  return (
    <div className="blueprint-root">
      {filmActive ? (
        <FilmControls
          time={filmTime}
          playing={filmPlaying}
          setPlaying={setFilmPlaying}
          speed={filmSpeed}
          setSpeed={setFilmSpeed}
          seek={setFilmTime}
          explore={() => {
            setExplorationEvidence({
              thread: filmFrame.thread,
              artifact: filmFrame.artifact,
              prs: filmFrame.prs,
            });
            setFilmActive(false);
            setFilmPlaying(false);
            history.replaceState(
              null,
              "",
              `?film=off&state=${screen === "thread" ? "thread-working" : screen}&thread=${selectedThread}`,
            );
          }}
          caption={filmFrame.title}
        />
      ) : (
        <ReviewBar
          scenario={scenario}
          load={load}
          guide={guide}
          setGuide={setGuide}
          reset={reset}
          demoStage={null}
          playDemo={startFilm}
          stopDemo={() => {}}
        />
      )}
      <div
        className={`review-workspace ${filmActive ? `film-stage ${!filmPlaying ? "film-paused" : ""}` : ""}`}
      >
        <div
          className={`app-shell ${!sidebar ? "sidebar-hidden" : ""}`}
          ref={appRef}
        >
          {sidebar ? (
            <Sidebar
              attentionCount={
                threadList.filter((t) =>
                  ["needs-you", "done"].includes(t.status),
                ).length
              }
              selectedThread={selectedThread}
              screen={screen}
              project={project}
              channel={channel}
              go={go}
              agent={agent}
              setAgent={setAgent}
              setSearch={setSearch}
              hide={() => setSidebar(false)}
            />
          ) : (
            <div className="collapsed-sidebar">
              <IconButton
                icon="sidebar"
                label="Show sidebar"
                onClick={() => setSidebar(true)}
              />
            </div>
          )}
          <main
            className={`main-space ${tools && screen === "thread" ? "with-tools" : ""}`}
            style={{ "--tools-width": `${toolWidth}%` }}
          >
            {conversation ? (
              <Conversation
                initialActivity={initialActivity}
                key={`${scenario}:${screen}:${selectedThread}:${agent}:${project}:${channel}`}
                screen={screen}
                project={project}
                channel={channel}
                status={screen === "thread" ? currentThread.status : status}
                setStatus={(s) => updateThread({ status: s })}
                filmFrame={filmFrame}
                threads={threadList.map((t) => ({
                  ...t,
                  activity: t.activity || activityAt(t, activityTime),
                }))}
                selectedThread={{
                  ...currentThread,
                  activity:
                    currentThread.activity ||
                    activityAt(currentThread, activityTime),
                }}
                openThread={openThread}
                updateThread={updateThread}
                go={go}
                agent={agent}
                tools={tools}
                setTools={setTools}
                messages={messages}
                send={send}
                draft={drafts[key] || ""}
                setDraft={(s) => setDrafts((d) => ({ ...d, [key]: s }))}
                scrollPositions={scrollPositions}
                notice={notice}
              />
            ) : (
              <WorkspacePages
                threads={threadList}
                openThread={openThread}
                key={screen}
                screen={screen}
                go={go}
                load={load}
                setAgent={setAgent}
              />
            )}
            {tools && screen === "thread" && (
              <>
                <div
                  className="resize-handle"
                  role="separator"
                  aria-label="Resize conversation and tools"
                  aria-orientation="vertical"
                  aria-valuemin={30}
                  aria-valuemax={60}
                  aria-valuenow={Math.round(toolWidth)}
                  tabIndex={0}
                  onPointerDown={resize}
                  onKeyDown={(e) => {
                    if (e.key === "ArrowLeft" || e.key === "ArrowRight") {
                      e.preventDefault();
                      setToolWidth((w) =>
                        Math.min(
                          60,
                          Math.max(30, w + (e.key === "ArrowLeft" ? 2 : -2)),
                        ),
                      );
                    }
                  }}
                />
                <Tools
                  key={selectedThread}
                  thread={currentThread}
                  go={go}
                  planTime={filmActive ? filmTime : activityTime}
                  messageCount={
                    (messages[`thread:${selectedThread}`] || []).length
                  }
                  filmArtifact={
                    filmFrame?.artifact ||
                    (explorationEvidence?.thread === selectedThread
                      ? explorationEvidence.artifact
                      : null)
                  }
                  filmPRs={filmFrame?.prs || explorationEvidence?.prs}
                  initialPR={selectedPR}
                  tab={toolTab}
                  setTab={setToolTab}
                  close={() => setTools(false)}
                  status={screen === "thread" ? currentThread.status : status}
                />
              </>
            )}
          </main>
        </div>
        {guide && (
          <Guide
            scenario={scenario}
            load={load}
            close={() => setGuide(false)}
          />
        )}
      </div>
      {filmActive && <FilmCursor frame={filmFrame} />}
      {search && (
        <div className="search-scrim" onClick={() => setSearch(false)}>
          <section
            className="search-dialog"
            role="dialog"
            aria-modal="true"
            aria-label="Search workspace"
            onClick={(e) => e.stopPropagation()}
          >
            <div className="search-field">
              <Icon name="search" />
              <input
                autoFocus
                aria-label="Search projects, conversations and agents"
                placeholder="Search projects, conversations, agents…"
                value={query}
                onChange={(e) => setQuery(e.target.value)}
              />
              <IconButton
                icon="close"
                label="Close search"
                onClick={() => setSearch(false)}
              />
            </div>
            <div className="search-results">
              {[
                ["Rebuild the workspace", "NuncioCrew · #product", "thread"],
                ["Product conversations", "NuncioCrew · channel", "channel"],
                ["Hermes", "Direct message", "dm"],
                ["Inbox", "Workspace", "inbox"],
                ["Wiki", "Workspace", "wiki"],
              ]
                .filter((x) =>
                  (x[0] + x[1]).toLowerCase().includes(query.toLowerCase()),
                )
                .map(([t, s, v]) => (
                  <button key={t} onClick={() => go(v)}>
                    <Icon
                      name={
                        v === "dm"
                          ? "message"
                          : v === "thread"
                            ? "file"
                            : "hash"
                      }
                    />
                    <div>
                      <strong>{t}</strong>
                      <span>{s}</span>
                    </div>
                    <Icon name="right" />
                  </button>
                ))}
            </div>
            <span className="search-footer">
              Esc to close · sample workspace
            </span>
          </section>
        </div>
      )}
      {toast && (
        <div className="toast" role="status">
          <Icon name="info" />
          {toast}
        </div>
      )}
    </div>
  );
}

export function App() {
  return (
    <AgentDirectoryProvider>
      <ChannelRolesProvider>
        <RecapProvider>
          <Workspace />
        </RecapProvider>
      </ChannelRolesProvider>
    </AgentDirectoryProvider>
  );
}

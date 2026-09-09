import { createContext, useContext, useState } from "react";
import { sampleProfiles, useAgentDirectory } from "./AgentDirectory";
const RecapContext = createContext(null);
const defaults = { runtime: "", model: "", profile: "" };
// Discovery fixtures only. Agent deletion does not uninstall a runtime.
export const sampleRuntimes = ["Hermes", "Codex", "Claude"];
export function RecapProvider({ children }) {
  const [settings, save] = useState(defaults);
  const [recaps, setRecaps] = useState({});
  return (
    <RecapContext.Provider
      value={{
        settings,
        save,
        recaps,
        setRecaps,
        reset: () => {
          save(defaults);
          setRecaps({});
        },
      }}
    >
      {children}
    </RecapContext.Provider>
  );
}
export const useRecap = () => useContext(RecapContext);
export function recapRuntimeLabel(settings) {
  return settings.runtime === "Hermes"
    ? `Hermes · ${settings.profile}`
    : `${settings.runtime} · ${settings.model || "Runtime default"}`;
}
export function RecapSettings() {
  const { settings, save } = useRecap();
  const [draft, setDraft] = useState(settings),
    [saved, setSaved] = useState(false);
  const patch = (p) => {
    setDraft((d) => ({ ...d, ...p }));
    setSaved(false);
  };
  return (
    <form
      className="recap-settings"
      onSubmit={(e) => {
        e.preventDefault();
        save({ ...draft, model: draft.model.trim() });
        setSaved(true);
      }}
    >
      <h2>Thread recap</h2>
      <p>
        Choose which runtime writes a short recap when you ask for one in Thread
        context.
      </p>
      <label>
        Recap runtime
        <select
          value={draft.runtime}
          onChange={(e) =>
            patch({ runtime: e.target.value, model: "", profile: "" })
          }
        >
          <option value="">Off — no generated recap</option>
          {sampleRuntimes.map((r) => (
            <option key={r}>{r}</option>
          ))}
        </select>
      </label>
      <p className="field-help">
        Available runtimes · sample inventory. Production will discover and
        verify your installed runtimes.
      </p>
      {draft.runtime === "Hermes" ? (
        <label>
          Hermes profile
          <select
            required
            aria-label="Hermes profile"
            value={draft.profile}
            onChange={(e) => patch({ profile: e.target.value })}
          >
            <option value="">Choose a profile</option>
            {sampleProfiles.map((p) => (
              <option key={p}>{p}</option>
            ))}
          </select>
          <span className="field-help">
            Model and provider come from the selected Hermes profile.
          </span>
        </label>
      ) : (
        draft.runtime && (
          <label>
            Recap model
            <input
              aria-label="Recap model"
              value={draft.model}
              placeholder="Runtime default"
              onChange={(e) => patch({ model: e.target.value })}
            />
            <span className="field-help">
              Optional model ID. Runtime access is not verified in this
              prototype.
            </span>
          </label>
        )
      )}
      <label>
        When to recap
        <select value="manual" onChange={() => {}}>
          <option value="manual">Only when I click Generate recap</option>
        </select>
      </label>
      <p>
        Activity and agent plans update directly from runtime events. This
        setting only affects generated recaps and does not change an agent’s
        working model.
      </p>
      <div className="settings-actions">
        <button
          type="button"
          onClick={() => {
            setDraft(settings);
            setSaved(false);
          }}
        >
          Discard changes
        </button>
        <button className="light-button" type="submit">
          Save settings
        </button>
      </div>
      {saved && <p role="status">Recap settings saved · sample only</p>}
    </form>
  );
}
export function ThreadRecap({ thread, go, messageCount = 0 }) {
  const { allAgents } = useAgentDirectory();
  const { settings, recaps, setRecaps } = useRecap();
  const recap = recaps[thread.id];
  const signature = JSON.stringify([
    thread.summary,
    thread.next,
    thread.status,
    messageCount,
    allAgents.map((a) => [a.id, a.name, Boolean(a.deleted)]),
  ]);
  const generate = () =>
    setRecaps((current) => ({
      ...current,
      [thread.id]: {
        text: `${thread.summary} ${thread.next || ""}`,
        source: recapRuntimeLabel(settings),
        signature,
        time: new Date().toLocaleTimeString([], {
          hour: "2-digit",
          minute: "2-digit",
        }),
      },
    }));
  return (
    <section className="thread-recap" aria-label="Thread recap">
      <h2>Recap</h2>
      {recap ? (
        <>
          <p>{recap.text}</p>
          <p className="field-help">
            Sample output · {recap.source} · {recap.time}
            {recap.signature !== signature ? " · Out of date" : ""}
          </p>
        </>
      ) : (
        <p>No recap generated yet.</p>
      )}
      <p className="field-help">
        {settings.runtime
          ? `Configured: ${recapRuntimeLabel(settings)}`
          : "Choose a runtime in Settings to enable recaps."}
      </p>
      <div className="settings-actions">
        <button disabled={!settings.runtime} onClick={generate}>
          {recap ? "Regenerate recap" : "Generate recap"}
        </button>
        <button onClick={() => go("settings")}>Recap settings</button>
      </div>
    </section>
  );
}

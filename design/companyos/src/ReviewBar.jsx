import { version, scenarios, layoutContract } from "./blueprint";
import { Icon, IconButton } from "./ui";
export function ReviewBar({
  scenario,
  load,
  guide,
  setGuide,
  reset,
  demoStage,
  playDemo,
  stopDemo,
}) {
  return (
    <>
      <div className="review-bar">
        <span className="review-brand">
          <Icon name="guide" size={14} /> CREW BLUEPRINT <span>v{version}</span>
          <span className="simulation-short">Simulated</span>
        </span>
        <span className="review-separator" />
        <span className="simulation-label">
          Interactive reference · simulated data
        </span>
        <a className="document-link" href="/index.html">
          Design document
        </a>
        <button
          className="motion-demo-toggle"
          onClick={demoStage === null ? playDemo : stopDemo}
        >
          <Icon name={demoStage === null ? "play" : "stop"} size={13} />
          {demoStage === null ? "Watch workspace film" : "End demo"}
        </button>
        <div className="spacer" />
        <label className="scenario-select">
          <span className="sr-only">Review scenario</span>
          <select
            aria-label="Review scenario"
            value={scenario}
            onChange={(e) => load(e.target.value)}
          >
            {scenarios.map((s) => (
              <option key={s.id} value={s.id}>
                {s.label}
              </option>
            ))}
          </select>
        </label>
        <IconButton icon="reset" label="Reset prototype" onClick={reset} />
        <button
          className={`review-toggle ${guide ? "active" : ""}`}
          onClick={() => setGuide(!guide)}
        >
          <Icon name="guide" size={14} />
          {guide ? "Close blueprint" : "Blueprint & notes"}
        </button>
      </div>
      {demoStage !== null && (
        <div className="motion-demo-bar" role="status">
          <span className="demo-label">SIMULATED · DELIVERY CHECKLIST</span>
          {["Drafting", "Verifying", "Ready for your review"].map((step, i) => (
            <span
              key={step}
              className={`demo-step ${demoStage === i ? "current" : ""} ${demoStage > i ? "complete" : ""}`}
            >
              <Icon
                name={
                  demoStage > i
                    ? "check"
                    : demoStage === i && i < 2
                      ? "spinner"
                      : "circle"
                }
                size={12}
              />
              {step}
            </span>
          ))}
          <span className="demo-note">
            {demoStage === 2
              ? "Waiting for your explicit acceptance"
              : "No agents or checks are actually running"}
          </span>
        </div>
      )}
    </>
  );
}
export function Guide({ scenario, load, close }) {
  const s = scenarios.find((x) => x.id === scenario) || scenarios[0];
  return (
    <aside className="guide" aria-label="Design blueprint">
      <header>
        <span className="eyebrow">DESIGN CONTRACT / 08 SEP 2026</span>
        <IconButton icon="close" label="Close blueprint" onClick={close} />
      </header>
      <h1>
        One workspace.
        <br />
        Three ways in.
      </h1>
      <p className="guide-intro">
        A living reference for the founder and implementation agents. Layout
        follows the selected Codex layout and accepted v0.9 Project/Wiki flow.
        See the Stage 0 matrix for remaining gates.
      </p>
      <h2>01 / What is decided</h2>
      <div className="contract-list">
        {layoutContract.map(([id, status, text]) => (
          <div key={id}>
            <span
              className={`contract-tag ${status.startsWith("Accepted") ? "accepted" : ""}`}
            >
              {id} · {status}
            </span>
            <p>{text}</p>
          </div>
        ))}
      </div>
      <h2>02 / Walk the states</h2>
      <div className="scenario-grid">
        {scenarios.map((x) => (
          <button
            key={x.id}
            className={s.id === x.id ? "active" : ""}
            onClick={() => load(x.id)}
          >
            {x.label}
            <Icon name="right" size={13} />
          </button>
        ))}
      </div>
      <div className="scenario-contract">
        <code>{s.id}</code>
        <h3>{s.intent}</h3>
        <ol>
          {s.steps.map((t) => (
            <li key={t}>{t}</li>
          ))}
        </ol>
        <h4>Expected behavior</h4>
        <ul>
          {s.expected.map((t) => (
            <li key={t}>{t}</li>
          ))}
        </ul>
        <h4>Existing Buzz seam</h4>
        <p>{s.seam}</p>
      </div>
      <h2>03 / Agent handoff</h2>
      <p>
        Use a stable state ID and the rules in <code>src/blueprint.js</code>.
        Read <code>README.md</code>, then Crew PRODUCT / DECISIONS. Layout
        accepted ≠ behavior accepted ≠ implemented.
      </p>
      <p>
        Projects in the sidebar supersede the navigation restriction in D-066
        for this design direction (D-078). A project is not automatically a
        repository, channel, worktree, or session. Resolve that mapping before
        production implementation.
      </p>
      <h2>04 / Source reference</h2>
      <p>
        Private source captures remain local and are excluded from this public
        handoff.
      </p>
      <a href="/index.html#authority">
        Stage 0 accepted / proposed / blocked matrix{" "}
        <Icon name="external" size={13} />
      </a>
      <p className="guide-footnote">
        No relay, runtime, files, PRs or messages are changed. Reload resets
        sample data. Review controls remain outside the app.
      </p>
    </aside>
  );
}

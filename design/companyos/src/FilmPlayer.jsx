import { useEffect, useState } from "react";
import { Icon } from "./ui";
import { FILM_DURATION, filmChapters } from "./film";
const clock = (t) =>
  `${Math.floor(t / 60)}:${String(Math.floor(t % 60)).padStart(2, "0")}`;
export function FilmControls({
  time,
  playing,
  setPlaying,
  speed,
  setSpeed,
  seek,
  explore,
  caption,
}) {
  return (
    <header className="film-controls">
      <div className="film-heading">
        <span className="film-label">WORKSPACE FILM · SIMULATED</span>
        <strong aria-live="polite" aria-atomic="true">
          {caption}
        </strong>
        <button onClick={explore}>
          Explore app
          <Icon name="out" size={12} />
        </button>
      </div>
      <div className="film-transport">
        <button
          aria-label={playing ? "Pause film" : "Play film"}
          onClick={() => setPlaying(!playing)}
        >
          <Icon name={playing ? "pause" : "play"} />
        </button>
        <button
          aria-label="Restart film"
          onClick={() => {
            seek(0);
            setPlaying(true);
          }}
        >
          <Icon name="reset" />
        </button>
        <time>{clock(time)}</time>
        <input
          aria-label="Film timeline"
          type="range"
          min="0"
          max={FILM_DURATION}
          step="0.1"
          value={time}
          onChange={(e) => seek(Number(e.target.value))}
        />
        <time>{clock(FILM_DURATION)}</time>
        <select
          aria-label="Playback speed"
          value={speed}
          onChange={(e) => setSpeed(Number(e.target.value))}
        >
          {[0.5, 1, 2, 4].map((n) => (
            <option key={n} value={n}>
              {n}×
            </option>
          ))}
        </select>
      </div>
      <nav className="film-chapters" aria-label="Film chapters">
        {filmChapters.map((c, i) => (
          <button
            key={c.at}
            aria-current={
              time >= c.at &&
              (i === filmChapters.length - 1 || time < filmChapters[i + 1].at)
                ? "step"
                : undefined
            }
            onClick={() => seek(c.at)}
          >
            {clock(c.at)} {c.title}
          </button>
        ))}
      </nav>
    </header>
  );
}
export function FilmCursor({ frame }) {
  const [point, setPoint] = useState({ x: 280, y: 180 });
  useEffect(() => {
    const target = document.querySelector(`[data-film="${frame.target}"]`);
    if (!target) return;
    const box = target.getBoundingClientRect();
    const next = {
      x: Math.max(
        12,
        Math.min(innerWidth - 120, box.x + Math.min(box.width * 0.62, 90)),
      ),
      y: Math.max(110, Math.min(innerHeight - 40, box.y + box.height * 0.5)),
    };
    setPoint((previous) =>
      previous.x === next.x && previous.y === next.y ? previous : next,
    );
  });
  return (
    <div
      className={`film-cursor ${frame.click ? "clicking" : ""}`}
      style={{ transform: `translate(${point.x}px,${point.y}px)` }}
      aria-hidden="true"
    >
      <Icon name="cursor" size={24} />
      <span>Oscar · story</span>
    </div>
  );
}
export function CampaignArtifact({ stage }) {
  return (
    <article className="document campaign-artifact">
      <span className="eyebrow">HEARDBACK / LAUNCH PACK · V3</span>
      <h1>Turn applications into conversations.</h1>
      <p className="doc-lead">
        A practical launch for people putting care into their next application.
      </p>
      <div className="campaign-tags">
        <span>Claude · Copy</span>
        <span>Hermes · Campaign lead</span>
      </div>
      <hr />
      {stage === "schedule" ? (
        <>
          <h2>Tuesday’s schedule</h2>
          <div className="schedule-slot">
            <strong>09:00</strong>
            <div>
              LinkedIn post<p>Claude · Approved copy v3</p>
            </div>
            <span>Prepared</span>
          </div>
          <div className="schedule-slot">
            <strong>10:00</strong>
            <div>
              Pilot email<p>Hermes · Small initial audience</p>
            </div>
            <span>Prepared</span>
          </div>
          <h3>After launch</h3>
          <p>
            Track visits and signups. Bring the first report back to the
            campaign thread.
          </p>
          <blockquote>
            Prepared is not performance. Results will come after the launch.
          </blockquote>
        </>
      ) : (
        <>
          <h2>Launch email</h2>
          <span className="eyebrow">SUBJECT</span>
          <h3>Your next application, with a clearer story.</h3>
          <p>
            You’ve put the work into your experience. HeardBack helps you
            present it for the role you actually want.
          </p>
          <p>
            Try it on one application. See what reads clearly, tighten what
            doesn’t, and send something you’re comfortable standing behind.
          </p>
          <div className="campaign-cta">Try HeardBack</div>
          <h2>LinkedIn</h2>
          <p>
            Good applications take care. We’re building HeardBack to help people
            tell a clearer story—and turn more applications into conversations.
          </p>
          <p>Our first pilot is opening Tuesday. We’d love your feedback.</p>
          {stage === "pack" && (
            <>
              <h2>Audience & evidence</h2>
              <ul>
                <li>Active job seekers tailoring applications.</li>
                <li>Start with a small pilot group.</li>
                <li>Signup tracking verified in the engineering thread.</li>
                <li>No promise of a guaranteed reply.</li>
              </ul>
            </>
          )}
        </>
      )}
      <div className="doc-footer">
        Campaign preparation · linked to the conversation
      </div>
    </article>
  );
}

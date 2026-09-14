import { useEffect, useRef } from "react";
import { Icon, IconButton } from "./ui";
import { wikiRevision, wikiSources } from "./wiki-content";

export function SourceCitations({ ids = [], onSource }) {
  if (!ids.length) return null;
  return (
    <div className="wiki-citations">
      <span>Sources</span>
      {ids.map((id) => {
        const source = wikiSources.find((s) => s.id === id);
        return (
          <button
            key={id}
            onClick={(event) => onSource(id, event.currentTarget)}
            aria-label={`Open source ${source.path.split("/").at(-1)} lines ${source.start} to ${source.end}`}
          >
            <Icon name="code" size={12} />
            <span>{source.path.split("/").at(-1)}</span>
            <small>
              {source.start}–{source.end}
            </small>
          </button>
        );
      })}
    </div>
  );
}

export function WikiToc({ pages, selected, onSelect }) {
  return (
    <nav className="wiki-toc" aria-label="Wiki contents">
      <span className="wiki-rail-label">CONTENTS</span>
      {[...new Set(pages.map((p) => p.section))].map((section) => (
        <section key={section}>
          <h2>{section}</h2>
          {pages
            .filter((p) => p.section === section)
            .map((page) => (
              <button
                key={page.id}
                className={selected === page.id ? "active" : ""}
                aria-current={selected === page.id ? "page" : undefined}
                onClick={() => onSelect(page.id)}
              >
                <span>{page.title}</span>
              </button>
            ))}
        </section>
      ))}
      <div className="wiki-toc-foot">
        <Icon name="wiki" size={14} />
        {pages.length} pages in this snapshot
      </div>
    </nav>
  );
}

export function WikiArticle({ page, record, onSource, onSelect, onScroll }) {
  const scroll = useRef(null);
  useEffect(() => {
    if (scroll.current) scroll.current.scrollTop = record.scroll[page.id] || 0;
  }, [page.id]);
  return (
    <div
      className="wiki-article-scroll"
      ref={scroll}
      onScroll={(e) => onScroll(page.id, e.currentTarget.scrollTop)}
    >
      <article className="wiki-article" aria-label={page.title}>
        <div className="wiki-article-eyebrow">
          <Icon name="wiki" size={14} />
          {page.section}
        </div>
        <h1>{page.title}</h1>
        <p className="wiki-article-description">{page.description}</p>
        <div className="wiki-article-meta">
          <span>Source snapshot</span>
          <code>{record.revision.slice(0, 7)}</code>
          <span>·</span>
          <span>{record.updated}</span>
        </div>
        {page.sections.map((section, index) => (
          <section key={section.title}>
            <h2>{section.title}</h2>
            <p>{section.text}</p>
            <SourceCitations ids={section.sources} onSource={onSource} />
            {index === 2 && page.comparison && (
              <div className="wiki-table-scroll">
                <table>
                  <caption>Git and folder workspace comparison</caption>
                  <thead>
                    <tr>
                      <th>Workspace</th>
                      <th>Git worktree</th>
                      <th>Plain folder</th>
                    </tr>
                  </thead>
                  <tbody>
                    {page.comparison.map(([label, git, folder]) => (
                      <tr key={label}>
                        <th>{label}</th>
                        <td>{git}</td>
                        <td>{folder}</td>
                      </tr>
                    ))}
                  </tbody>
                </table>
              </div>
            )}
          </section>
        ))}
        {!!page.related?.length && (
          <footer className="wiki-related">
            <h2>Keep exploring</h2>
            {page.related.map((id) => {
              const related = record.pages.find((p) => p.id === id);
              return (
                <button key={id} onClick={() => onSelect(id)}>
                  <Icon name="file" size={15} />
                  {related.title}
                  <Icon name="right" size={14} />
                </button>
              );
            })}
          </footer>
        )}
      </article>
    </div>
  );
}

export function WikiSourcePanel({ id, onClose, onNotice }) {
  const source = wikiSources.find((s) => s.id === id);
  const close = useRef(null),
    highlighted = useRef(null);
  useEffect(() => {
    close.current?.focus();
    highlighted.current?.scrollIntoView({ block: "center" });
  }, [id]);
  return (
    <aside
      className="wiki-source"
      aria-label="Source excerpt"
      onKeyDown={(e) => {
        if (e.key === "Escape") {
          e.preventDefault();
          e.stopPropagation();
          onClose();
        }
      }}
    >
      <header>
        <div>
          <span className="eyebrow">SOURCE SNAPSHOT</span>
          <h2>{source.path.split("/").at(-1)}</h2>
        </div>
        <button
          ref={close}
          className="icon-button"
          aria-label="Close source"
          onClick={onClose}
        >
          <Icon name="close" />
        </button>
      </header>
      <div className="wiki-source-path">{source.path}</div>
      <div className="wiki-source-meta">
        <Icon name="branch" size={13} />
        main <code>{wikiRevision.slice(0, 7)}</code>
        <span>
          Lines {source.start}–{source.end}
        </span>
      </div>
      <div className="wiki-code-scroll">
        <pre>
          <code>
            {source.lines.map((line) => (
              <span
                key={line.number}
                ref={line.number === source.start ? highlighted : null}
                className={`wiki-code-line ${line.number >= source.start && line.number <= source.end ? "highlighted" : ""}`}
              >
                <span className="wiki-line-number" aria-hidden="true">
                  {line.number}
                </span>
                <span>{line.text || " "}</span>
              </span>
            ))}
          </code>
        </pre>
      </div>
      <footer>
        <span>Captured source · read only</span>
        <div>
          <button
            onClick={async () => {
              try {
                await navigator.clipboard.writeText(source.path);
                onNotice("Source path copied");
              } catch {
                onNotice("Copy unavailable. The full path is shown above.");
              }
            }}
          >
            Copy path
          </button>
          <a
            href={`https://github.com/Nuncio-hq/crew/blob/${wikiRevision}/${source.path}#L${source.start}-L${source.end}`}
            target="_blank"
            rel="noreferrer"
          >
            GitHub <Icon name="external" size={13} />
          </a>
        </div>
      </footer>
    </aside>
  );
}

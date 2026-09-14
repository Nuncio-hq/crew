import sources from "./wiki-sources.json";

export const wikiRevision = "8278d2b14cd04e17c1010d819c9fdd20eaefff9a";
export const wikiSources = sources;
export const wikiPages = [
  {
    id: "overview",
    section: "Getting oriented",
    title: "NuncioCrew overview",
    description:
      "Projects, conversations and agents, connected to the same work.",
    keywords: "project repository architecture overview crew intro",
    sections: [
      {
        title: "Start with a conversation",
        text: "A Project brings related repositories and channels together. Channels hold the conversations; a thread gives a particular piece of work its own context. Opening a conversation does not, by itself, create a new agent session or checkout.",
      },
      {
        title: "Connect the work to its source",
        text: "When an agent needs a workspace, Crew tracks the repository path, working directory and source revision alongside the thread root. This lets the conversation point back to the files used for the work.",
        sources: ["thread"],
      },
      {
        title: "Read with a revision in view",
        text: "A repository Wiki stores the commit and branch used to generate its contents. Pages retain their source file list, so a description can be checked against the indexed snapshot.",
        sources: ["wiki"],
      },
    ],
    related: ["workspaces", "wiki"],
  },
  {
    id: "workspaces",
    section: "Working with code",
    title: "Thread workspaces",
    description:
      "How a conversation gets a working directory — and what is actually isolated.",
    keywords:
      "thread workspace isolated isolation directory conversation worktree folder git how different",
    sections: [
      {
        title: "From a thread to a workspace",
        text: "Crew associates workspace metadata with the root of a conversation thread. The record contains the repository path, working directory, branch and base revision. A thread can therefore keep its code context while you move between conversations.",
        sources: ["thread"],
      },
      {
        title: "Git worktrees",
        text: "For a dedicated Git checkout, Crew creates a worktree and a branch from the selected base revision. Files can change in that checkout without changing the files in the main checkout. The worktrees still share Git objects and common repository metadata.",
        sources: ["git"],
      },
      {
        title: "Plain folders work differently",
        text: "A folder workspace uses the canonical folder itself as the agent’s working directory. It does not create a separate branch or copy of the folder for each thread. Local version history is kept in a shadow repository outside the working folder.",
        sources: ["folder", "history"],
      },
      {
        title: "Plan first, then claim the workspace",
        text: "The discovery plan identifies paths and the base revision before creating or attaching a checkout. The documented lifecycle acquires a lease against the common Git directory and thread root before ensuring the planned worktree.",
        sources: ["plan"],
      },
      {
        title: "Check the checkout mode",
        text: "Dedicated worktrees are one mode. Existing branches and the main checkout are also supported by the underlying workspace model. A plain folder is shared, so “one thread” should never be read as a promise of filesystem isolation in every mode.",
        sources: ["thread", "folder"],
      },
    ],
    comparison: [
      ["Working directory", "Dedicated checkout", "The linked folder itself"],
      ["Branch", "Created or selected Git branch", "No per-thread branch"],
      ["History", "Repository Git history", "External shadow repository"],
      [
        "Isolation",
        "Separate working files; shared Git metadata",
        "Shared folder; access coordinated separately",
      ],
    ],
    related: ["git", "folders", "leases"],
  },
  {
    id: "git",
    section: "Working with code",
    title: "Git worktrees & branches",
    description:
      "Create a checkout from a known base and keep its identity with the thread.",
    keywords: "git worktree branch checkout base commit revision main create",
    sections: [
      {
        title: "A known starting point",
        text: "The workspace plan carries a base revision, a branch and a target worktree path. These are chosen before the checkout is created.",
        sources: ["plan"],
      },
      {
        title: "Create the dedicated checkout",
        text: "The creation path runs git worktree add -b with the branch, target directory and base revision. This creates separate working files while retaining the shared Git repository.",
        sources: ["git"],
      },
      {
        title: "Keep the shared boundary visible",
        text: "The workspace record stores common_git alongside the per-thread path. Repository metadata is shared even when the checked-out files are separate.",
        sources: ["thread"],
      },
    ],
    related: ["workspaces", "leases"],
  },
  {
    id: "folders",
    section: "Working with code",
    title: "Plain-folder workspaces",
    description: "Work with a folder without creating a Git branch inside it.",
    keywords:
      "folder plain cowork shared directory shadow history documents no git difference",
    sections: [
      {
        title: "Use the existing folder",
        text: "Folder planning canonicalizes the configured path and checks that it is a directory. The planned repository path and working path both point to this same folder.",
        sources: ["folder"],
      },
      {
        title: "Keep history outside it",
        text: "Crew opens or initializes a shadow repository in the configured history location. This provides local version history without turning the user’s folder into a normal per-thread Git worktree.",
        sources: ["history"],
      },
      {
        title: "A shared working directory",
        text: "Two threads using this folder do not receive isolated file copies. The folder plan records CheckoutKind::Folder and uses the shadow repository’s Git directory as its common identity.",
        sources: ["folder"],
      },
    ],
    related: ["workspaces", "git"],
  },
  {
    id: "leases",
    section: "Agent execution",
    title: "Workspace planning & leases",
    description:
      "Separate discovery from the operation that claims or creates a checkout.",
    keywords:
      "lease lock claim execution concurrency plan agent session workspace safety",
    sections: [
      {
        title: "Discovery does not create a checkout",
        text: "ThreadWorkspacePlan holds identity paths and the base revision. Producing this plan does not create, reattach or claim the checkout.",
        sources: ["plan"],
      },
      {
        title: "Claim before ensuring",
        text: "The plan’s contract requires the shared active-turn lease before ensuring the checkout. The common Git directory and thread root identify the lease boundary.",
        sources: ["plan"],
      },
      {
        title: "Keep identity, path and history separate",
        text: "The thread root, worktree path and common Git directory answer different questions: which conversation owns the context, where working files live, and which repository metadata is shared.",
        sources: ["thread"],
      },
    ],
    related: ["workspaces", "folders"],
  },
  {
    id: "wiki",
    section: "Project knowledge",
    title: "Wiki pages & source snapshots",
    description:
      "Understand which revision a page describes before using it to make a change.",
    keywords:
      "wiki update generation generate index page source citation commit snapshot cadence",
    sections: [
      {
        title: "The table of contents",
        text: "The Wiki table of contents records repository identity, owner, commit, branch, cadence and generation time. Its sections identify the pages in the indexed snapshot.",
        sources: ["wiki"],
      },
      {
        title: "Every page has a source",
        text: "A Wiki page keeps its repository key, slug, title, section, commit, language, source files and content. Citations in this preview open a captured source excerpt at the displayed revision.",
        sources: ["wiki"],
      },
      {
        title: "Updates can fail",
        text: "The current Wiki job model distinguishes idle, generating and failed states and includes progress counts and an error. A failed update should leave the last available snapshot readable.",
        sources: ["job"],
      },
    ],
    related: ["overview", "workspaces"],
  },
];

export function pageText(page) {
  return [
    page.title,
    page.description,
    ...page.sections.flatMap((s) => [s.title, s.text]),
  ].join(" ");
}
export function searchWiki(pages, query) {
  const q = query.trim().toLowerCase();
  if (!q) return pages.map((page) => ({ page, excerpt: page.description }));
  return pages.flatMap((page) => {
    const text = pageText(page),
      index = text.toLowerCase().indexOf(q);
    return index < 0
      ? []
      : [
          {
            page,
            excerpt: `${index > 60 ? "…" : ""}${text.slice(Math.max(0, index - 60), index + q.length + 140)}…`,
          },
        ];
  });
}
export function sampleWikiAnswer(question, pages, pageId) {
  const q = question.toLowerCase();
  let target = /folder|plain|cowork|thư mục|khác/.test(q)
    ? "folders"
    : /wiki|index|generat|citation/.test(q)
      ? "wiki"
      : /lease|lock|claim/.test(q)
        ? "leases"
        : /workspace|worktree|thread|isolat|không gian|luồng/.test(q)
          ? "workspaces"
          : null;
  if (!target && /this page|explain|source|file|trang này/.test(q))
    target = pageId;
  const page = pages.find((p) => p.id === target);
  if (!page) return { missing: true, paragraphs: [], pageId, sources: [] };
  return {
    pageId: page.id,
    paragraphs: page.sections.slice(0, 3),
    sources: [
      ...new Set(page.sections.slice(0, 3).flatMap((s) => s.sources || [])),
    ],
  };
}

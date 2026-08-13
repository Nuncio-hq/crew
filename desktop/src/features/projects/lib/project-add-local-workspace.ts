import {
  linkProjectWorkspaceTags,
  validateLocalWorkspacePath,
  withCrewWorkspaceMode,
} from "./project-local-workspace";

export type ExistingLocalWorkspaceProject<Saved> = {
  channelId: string | null;
  dtag: string;
  localPath: string | null;
  owner: string;
  saved: Saved;
};

export type ProjectChannelRetry = {
  channelId: string;
  dtag: string;
  owner: string;
};

export type LocalWorkspaceProjectInput = {
  localPath: string;
  name: string;
  retryChannel?: ProjectChannelRetry | null;
  workspaceMode?: "git" | "folder";
};

export type LocalWorkspaceProjectDraft = {
  content: string;
  dtag: string;
  tags: string[][];
};

type CreateDependencies<Saved> = {
  createChannel: (projectName: string) => Promise<string>;
  findProject: (
    owner: string,
    dtag: string,
  ) => Promise<ExistingLocalWorkspaceProject<Saved> | null>;
  getOwnerPubkey: () => Promise<string>;
  publishAndReadBack: (input: {
    channelId: string;
    draft: LocalWorkspaceProjectDraft;
    localPath: string;
    owner: string;
  }) => Promise<Saved>;
};

export class ProjectLocalWorkspaceCreateError extends Error {
  readonly retryChannel: ProjectChannelRetry;

  constructor(cause: unknown, retryChannel: ProjectChannelRetry) {
    super(
      cause instanceof Error ? cause.message : "Could not create Repository.",
      {
        cause,
      },
    );
    this.name = "ProjectLocalWorkspaceCreateError";
    this.retryChannel = retryChannel;
  }
}

export function projectDtagFromName(name: string): string {
  return name
    .trim()
    .toLowerCase()
    .replace(/[^a-z0-9]+/g, "-")
    .replace(/^-+|-+$/g, "");
}

export function projectNameFromLocalPath(localPath: string): string {
  const path = validateLocalWorkspacePath(localPath).replace(/\/+$/u, "");
  const name = path.slice(path.lastIndexOf("/") + 1);
  if (!name) throw new Error("Choose a local folder with a name.");
  return name;
}

export function buildLocalWorkspaceProject(input: {
  channelId: string;
  localPath: string;
  name: string;
  workspaceMode?: "git" | "folder";
}): LocalWorkspaceProjectDraft {
  const name = input.name.trim();
  const dtag = projectDtagFromName(name);
  if (!dtag) {
    throw new Error("Repository name must include letters or numbers.");
  }
  if (!input.channelId.trim()) {
    throw new Error("Project channel is required.");
  }
  const tags = withCrewWorkspaceMode(
    linkProjectWorkspaceTags(
      [
        ["d", dtag],
        ["name", name],
      ],
      { channelId: input.channelId, localPath: input.localPath },
    ),
    input.workspaceMode ?? "git",
  );
  return { content: "", dtag, tags };
}

export async function createLocalWorkspaceProject<Saved>(
  input: LocalWorkspaceProjectInput,
  dependencies: CreateDependencies<Saved>,
): Promise<{ channelId: string; dtag: string; saved: Saved }> {
  validateLocalWorkspacePath(input.localPath);
  const dtag = projectDtagFromName(input.name);
  if (!dtag) {
    throw new Error("Repository name must include letters or numbers.");
  }
  const owner = (await dependencies.getOwnerPubkey()).toLowerCase();
  const existing = await dependencies.findProject(owner, dtag);
  const retryMatchesIdentity =
    input.retryChannel?.owner.toLowerCase() === owner &&
    input.retryChannel.dtag === dtag;
  if (
    existing &&
    retryMatchesIdentity &&
    existing.owner.toLowerCase() === owner &&
    existing.dtag === dtag &&
    existing.channelId === input.retryChannel?.channelId &&
    existing.localPath === input.localPath
  ) {
    return {
      channelId: existing.channelId,
      dtag,
      saved: existing.saved,
    };
  }
  if (existing) {
    throw new Error(`You already have a Repository named "${dtag}".`);
  }

  const reusableChannelId = retryMatchesIdentity
    ? input.retryChannel?.channelId
    : null;
  const channelId =
    reusableChannelId ?? (await dependencies.createChannel(input.name.trim()));
  const retryChannel = { channelId, dtag, owner };
  const draft = buildLocalWorkspaceProject({
    channelId,
    localPath: input.localPath,
    name: input.name,
    workspaceMode: input.workspaceMode,
  });

  try {
    const saved = await dependencies.publishAndReadBack({
      channelId,
      draft,
      localPath: input.localPath,
      owner,
    });
    return { channelId, dtag, saved };
  } catch (error) {
    throw new ProjectLocalWorkspaceCreateError(error, retryChannel);
  }
}

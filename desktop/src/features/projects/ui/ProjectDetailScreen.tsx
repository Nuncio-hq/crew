import * as React from "react";
import { toast } from "sonner";

import { useAppNavigation } from "@/app/navigation/useAppNavigation";
import { useOpenDmMutation } from "@/features/channels/hooks";
import {
  type Project,
  type Repository,
  useProjectQuery,
  useProjectIssuesQuery,
  useProjectLocalRepoDiffQuery,
  useProjectLocalRepoSnapshotQuery,
  useProjectRepoDiffQuery,
  useProjectPullRequestsQuery,
  useProjectsWorkItemsQuery,
  useProjectRepoSnapshotQuery,
  useProjectsQuery,
  useRepoStateQuery,
} from "@/features/projects/hooks";
import {
  useCloneProjectRepositoryMutation,
  useProjectRepoSyncStatusQuery,
  usePullProjectLocalRepositoryMutation,
  usePushProjectLocalRepositoryMutation,
} from "@/features/projects/repoSyncHooks";
import { useProjectBranchActions } from "@/features/projects/branchMutations";
import { useOptimisticProjectBranches } from "@/features/projects/useOptimisticProjectBranches";
import { useProjectRepositoryRefSelection } from "@/features/projects/useProjectRepositoryRefSelection";
import { useUpdateProjectPullRequestMutation } from "@/features/projects/pullRequestMutations";
import { useCreateProjectIssueMutation } from "@/features/projects/issueMutations";
import { useProfileQuery, useUsersBatchQuery } from "@/features/profile/hooks";
import { mergeCurrentProfileIntoLookup } from "@/features/profile/lib/identity";
import {
  type ProfilePanelTab,
  type ProfilePanelView,
  UserProfilePanel,
} from "@/features/profile/ui/UserProfilePanel";
import {
  profilePanelTabFromSearch,
  profilePanelViewFromSearch,
} from "@/features/profile/ui/UserProfilePanelUtils";
import { useIdentityQuery } from "@/shared/api/hooks";
import { openProjectMergeRecoveryTerminal } from "@/shared/api/projectGit";
import { useMainInsetRef } from "@/shared/layout/MainInsetContext";
import { channelContentTopPaddingMeasurement } from "@/shared/layout/chromeLayout";
import { useMeasuredCssVariable } from "@/shared/layout/useMeasuredCssVariable";
import { ProfilePanelProvider } from "@/shared/context/ProfilePanelContext";
import { useHistorySearchState } from "@/shared/hooks/useHistorySearchState";
import { useThreadPanelWidth } from "@/shared/hooks/useThreadPanelWidth";

import { useCommunities } from "@/features/communities/useCommunities";
import { useProjectCommitDiffQuery } from "@/features/projects/useProjectCommitDiff";
import { useGitIdentityQuery } from "@/features/projects/useGitIdentity";
import type { ViewerGitIdentity } from "@/features/projects/lib/projectContributorMatching";
import {
  projectBranchCreationReason,
  projectBranchManagementState,
  projectBranchOptionsFromSync,
  resolveProjectDefaultBranch,
} from "@/features/projects/lib/projectBranches";
import { localWorkspaceSourceState } from "@/features/projects/lib/project-exact-local-workspace";
import {
  cloneUrlList,
  firstCloneUrl,
} from "@/features/projects/lib/projectCloneUrl";
import { normalizeRepositoryUrl } from "@/features/projects/lib/projectsViewHelpers";
import { selectProjectRepository } from "@/features/projects/projectModels";
import { KIND_REPO_ANNOUNCEMENT } from "@/shared/constants/kinds";
import { useProjectRepoPresentation } from "@/features/projects/useProjectRepoHost";
import { WorkspaceTabs } from "./ProjectWorkspaceTabs";
import { ProjectOutcomeDetail } from "./ProjectOutcomeDetail";
import type { RepoSourceHeaderControls } from "./ProjectRepositorySource";
import { showProjectCloneErrorToast } from "./projectGitErrorToast";
import {
  projectTerminalLabel,
  useOpenProjectTerminal,
} from "./useOpenProjectTerminal";
import type { CreateIssueDialogInput } from "./CreateIssueDialog";
import { ProjectBranchActionDialogs } from "./ProjectBranchActionDialogs";
import { ProjectDetailChrome } from "./ProjectDetailChrome";
import {
  buildProjectDetailWorkItemCrumb,
  ProjectDetailRepositoryHeader,
} from "./project-detail-navigation";
import {
  ProjectDetailScreenError,
  ProjectDetailScreenLoading,
  ProjectDetailScreenNoRepository,
  ProjectDetailScreenNotFound,
} from "./project-detail-screen-states";
import {
  PROJECT_TAB_CRUMB_LABELS,
  projectPeople,
  pushPullTitle,
  snapshotHasContent,
} from "./projectDetailHelpers";

type ProjectDetailScreenProps = {
  commitHash?: string;
  projectId: string;
  pullRequestId?: string;
  issueId?: string;
  repositoryId?: string;
};

const PROJECT_DETAIL_PANEL_SEARCH_KEYS = [
  "profile",
  "profileTab",
  "profileView",
] as const;
const PROJECT_REPOSITORY_SEARCH_KEYS = [
  "repositoryId",
  "issueId",
  "pullRequestId",
  "commitHash",
] as const;

export function ProjectDetailScreen(props: ProjectDetailScreenProps) {
  const { commitHash, projectId, pullRequestId, issueId, repositoryId } = props;
  const { goChannel, goProject, goProjects } = useAppNavigation();
  const { activeCommunity } = useCommunities();
  const mainInsetRef = useMainInsetRef();
  const projectDetailHeaderChromeRef = useMeasuredCssVariable({
    targetRef: mainInsetRef,
    resetKey: projectId,
    ...channelContentTopPaddingMeasurement,
  });
  const projectQuery = useProjectQuery(projectId);
  const projectsQuery = useProjectsQuery();
  const project = projectQuery.data;
  const projectWorkItemsQuery = useProjectsWorkItemsQuery(
    project ? [project] : [],
  );
  // When the projectId is a canonical 30617:<owner>:<d> coordinate (emitted by
  // entity links in #4695), derive the repository selection directly from the
  // <owner>:<d> portion rather than falling back to the project's primary
  // repository. Repository.id is "<owner>:<dtag>", so stripping the kind+colon
  // prefix gives the exact repository id. This ensures a linked PR/issue on a
  // non-primary member opens from the correct repository instead of the primary.
  const routeRepositoryId: string | undefined = React.useMemo(() => {
    if (repositoryId) return repositoryId;
    const kindStr = `${String(KIND_REPO_ANNOUNCEMENT)}:`;
    if (!projectId.startsWith(kindStr)) return undefined;
    // projectId is "30617:<owner>:<dtag>" — strip "30617:" to get "<owner>:<dtag>"
    return projectId.slice(kindStr.length);
  }, [projectId, repositoryId]);
  const repository = selectProjectRepository(project, routeRepositoryId);
  const isLinkedWorkspace =
    Boolean(repository?.localWorkspacePath) ||
    repository?.localWorkspaceStatus === "invalid";
  const repoRemote = useProjectRepoPresentation(repository);
  const { applyPatch: applyRepositorySearch } = useHistorySearchState(
    PROJECT_REPOSITORY_SEARCH_KEYS,
  );
  const repoStateQuery = useRepoStateQuery(repository);
  const pullRequestsQuery = useProjectPullRequestsQuery(repository);
  const defaultBranch = repository
    ? resolveProjectDefaultBranch(repository.defaultBranch, repoStateQuery.data)
    : null;
  const { branchOptions, forgetBranch, managedBranches, rememberBranch } =
    useOptimisticProjectBranches({
      defaultBranch,
      observedBranches: repoStateQuery.data?.branches ?? [],
      projectId: repository?.id ?? projectId,
      referencedBranches:
        pullRequestsQuery.data?.map(
          (pullRequest) => pullRequest.branchName ?? null,
        ) ?? [],
    });
  const { activeBranch, selectBranch, selectedTag, selectTag } =
    useProjectRepositoryRefSelection({
      branchOptions,
      defaultBranch,
      projectAvailable: Boolean(repository),
      projectPending: projectQuery.isPending,
      tags: repoStateQuery.data?.tags ?? [],
    });
  const activeTag =
    repoStateQuery.data?.tags.find((tag) => tag.name === selectedTag) ?? null;
  const [selectedPullRequestId, setSelectedPullRequestId] = React.useState<
    string | null
  >(pullRequestId ?? null);
  React.useEffect(
    () => setSelectedPullRequestId(pullRequestId ?? null),
    [pullRequestId],
  );
  const [selectedIssueId, setSelectedIssueId] = React.useState<string | null>(
    issueId ?? null,
  );
  React.useEffect(() => setSelectedIssueId(issueId ?? null), [issueId]);
  const [selectedCommitHash, setSelectedCommitHash] = React.useState<
    string | null
  >(commitHash ?? null);
  React.useEffect(
    () => setSelectedCommitHash(commitHash ?? null),
    [commitHash],
  );
  // Bumped when breadcrumb navigation should land on the project Overview
  // tab; remounts WorkspaceTabs, which owns the selected-tab state.
  const [tabsResetKey, setTabsResetKey] = React.useState(0);
  // Mirror of the WorkspaceTabs selection so the breadcrumb can name the
  // active sub-tab. The Overview (readme) tab is "home" and gets no crumb.
  const [activeTab, setActiveTab] = React.useState("overview");
  // Commit, PR, and issue details are mutually exclusive views, so opening
  // one clears the others.
  const handleSelectedPullRequestIdChange = React.useCallback(
    (id: string | null) => {
      setSelectedPullRequestId(id);
      if (id) setSelectedCommitHash(null);
    },
    [],
  );
  const handleSelectedIssueIdChange = React.useCallback((id: string | null) => {
    setSelectedIssueId(id);
    if (id) setSelectedCommitHash(null);
  }, []);
  const handleSelectedCommitHashChange = React.useCallback(
    (hash: string | null) => {
      setSelectedCommitHash(hash);
      if (hash) {
        setSelectedPullRequestId(null);
        setSelectedIssueId(null);
      }
    },
    [],
  );
  const issuesQuery = useProjectIssuesQuery(repository);
  const selectedBranchPullRequest = React.useMemo(() => {
    const projectRepositories = new Set(
      cloneUrlList(repository).map(normalizeRepositoryUrl),
    );
    const matches =
      pullRequestsQuery.data?.filter(
        (pullRequest) =>
          pullRequest.branchName === activeBranch &&
          pullRequest.cloneUrls.some((cloneUrl) =>
            projectRepositories.has(normalizeRepositoryUrl(cloneUrl)),
          ),
      ) ?? [];
    return matches.length === 1 ? matches[0] : null;
  }, [activeBranch, pullRequestsQuery.data, repository]);
  const openBranchPullRequest =
    selectedBranchPullRequest?.status === "Open" ||
    selectedBranchPullRequest?.status === "Draft"
      ? selectedBranchPullRequest
      : null;
  const activeRepoPullRequest =
    pullRequestsQuery.data?.find((item) => item.id === selectedPullRequestId) ??
    selectedBranchPullRequest;
  const [repoSource, setRepoSource] = React.useState<"remote" | "local">(
    "remote",
  );
  const effectiveRepoSource = isLinkedWorkspace ? "local" : repoSource;
  const repoSnapshotQuery = useProjectRepoSnapshotQuery(
    repository,
    activeBranch,
    selectedTag ? null : selectedBranchPullRequest,
    activeTag,
    repoRemote.host.kind === "buzz",
  );
  const repoDiffQuery = useProjectRepoDiffQuery(
    repository,
    activeBranch,
    activeRepoPullRequest,
    effectiveRepoSource === "remote",
  );
  const localRepoDiffQuery = useProjectLocalRepoDiffQuery(
    repository,
    activeCommunity?.reposDir,
    activeBranch,
    activeRepoPullRequest,
    effectiveRepoSource === "local" && Boolean(activeRepoPullRequest),
  );
  const commitDiffQuery = useProjectCommitDiffQuery(
    repository,
    selectedCommitHash,
    effectiveRepoSource,
    activeCommunity?.reposDir,
  );
  const localRepoSnapshotQuery = useProjectLocalRepoSnapshotQuery(
    repository,
    activeCommunity?.reposDir,
    activeBranch,
    selectedTag,
  );
  const repoSyncStatusQuery = useProjectRepoSyncStatusQuery(
    repository,
    activeCommunity?.reposDir,
    activeBranch,
  );
  const pushLocalRepoMutation = usePushProjectLocalRepositoryMutation(
    repository,
    activeCommunity?.reposDir,
    activeBranch,
    openBranchPullRequest,
  );
  const pullLocalRepoMutation = usePullProjectLocalRepositoryMutation(
    repository,
    activeCommunity?.reposDir,
    activeBranch,
  );
  const cloneRepoMutation = useCloneProjectRepositoryMutation(
    repository,
    activeCommunity?.reposDir,
  );
  const createIssueMutation = useCreateProjectIssueMutation(repository);
  const updatePullRequestMutation = useUpdateProjectPullRequestMutation(
    repository,
    openBranchPullRequest,
  );
  const hasLocalCheckout = Boolean(
    localRepoSnapshotQuery.data || repoSyncStatusQuery.data?.localPath,
  );
  const canOpenTerminal =
    !isLinkedWorkspace &&
    Boolean(hasLocalCheckout || firstCloneUrl(repository));
  const localSource = localWorkspaceSourceState({
    hasSnapshot: hasLocalCheckout,
    isError: localRepoSnapshotQuery.isError,
    isLinked: isLinkedWorkspace,
    isLoading: localRepoSnapshotQuery.isLoading,
    isTagSelected: Boolean(selectedTag),
  });
  const hasRemoteSnapshot = snapshotHasContent(repoSnapshotQuery.data);
  const displayedRepoDiff =
    effectiveRepoSource === "local"
      ? localRepoDiffQuery.data
      : repoDiffQuery.data;
  const displayedRepoDiffError =
    effectiveRepoSource === "local"
      ? localRepoDiffQuery.error
      : repoDiffQuery.error;
  const displayedRepoDiffLoading =
    effectiveRepoSource === "local"
      ? localRepoDiffQuery.isLoading
      : repoDiffQuery.isLoading;
  const branchOptionsWithLocal = projectBranchOptionsFromSync(
    branchOptions,
    repoSyncStatusQuery.data,
  );
  const { activeBranchCommit, activeRemoteBranch, deleteBranchReason } =
    projectBranchManagementState({
      activeBranch,
      branches: managedBranches,
      defaultBranch,
      hasOpenPullRequest: (pullRequestsQuery.data ?? []).some(
        (pullRequest) =>
          pullRequest.branchName === activeBranch &&
          (pullRequest.status === "Open" || pullRequest.status === "Draft"),
      ),
      remoteBranch: repoSyncStatusQuery.data?.remoteBranch,
      remoteHead: repoSyncStatusQuery.data?.remoteHead,
      snapshotCommit: repoSnapshotQuery.data?.latestCommit?.hash,
    });
  const handleBranchChange = React.useCallback(
    (branch: string | null) => {
      selectBranch(branch);
      if (
        branch &&
        effectiveRepoSource === "local" &&
        !isLinkedWorkspace &&
        branch !== repoSyncStatusQuery.data?.localBranch
      ) {
        setRepoSource("remote");
      }
    },
    [
      isLinkedWorkspace,
      effectiveRepoSource,
      repoSyncStatusQuery.data?.localBranch,
      selectBranch,
    ],
  );
  const handleTagChange = React.useCallback(
    (tag: string) => {
      selectTag(tag);
      setRepoSource("remote");
    },
    [selectTag],
  );
  const branchActions = useProjectBranchActions({
    activeBranch,
    activeBranchCommit,
    activeRemoteBranch,
    defaultBranch,
    deleteBranchReason,
    forgetBranch,
    project: repository,
    refetchRepoState: repoStateQuery.refetch,
    rememberBranch,
    selectBranch: handleBranchChange,
  });
  const createBranchReason = projectBranchCreationReason({
    activeBranch,
    activeBranchCommit,
    localHead: repoSyncStatusQuery.data?.localHead,
  });
  const handleFetchRepo = React.useCallback(async () => {
    const results = await Promise.all([
      repoSnapshotQuery.refetch(),
      repoStateQuery.refetch(),
      repoSyncStatusQuery.refetch(),
    ]);
    const error = results.find((result) => result.error)?.error;
    if (error) {
      toast.error("Could not fetch repository.", {
        description:
          error instanceof Error ? error.message : "The Git fetch failed.",
      });
      return;
    }
    toast.success("Remote state refreshed.");
  }, [repoSnapshotQuery, repoStateQuery, repoSyncStatusQuery]);
  // Compact branch + remote/local controls shared by the readme and Files
  // tab headers.
  const filesSourceControls: RepoSourceHeaderControls = {
    branch: activeBranch ?? "",
    branchOptions: branchOptionsWithLocal,
    selectedTag,
    tagOptions: repoStateQuery.data?.tags ?? [],
    onBranchChange: handleBranchChange,
    onTagChange: isLinkedWorkspace ? undefined : handleTagChange,
    onCreateBranch: isLinkedWorkspace
      ? undefined
      : () => branchActions.setCreateOpen(true),
    createBranchDisabled: branchActions.createPending || !activeBranchCommit,
    createBranchTitle: createBranchReason ?? "Create a remote branch",
    onDeleteBranch: isLinkedWorkspace
      ? undefined
      : () => branchActions.setDeleteOpen(true),
    deleteBranchDisabled:
      branchActions.deletePending || Boolean(deleteBranchReason),
    deleteBranchTitle: deleteBranchReason ?? "Delete this remote branch",
    source: isLinkedWorkspace ? "local" : selectedTag ? "remote" : repoSource,
    onSourceChange: isLinkedWorkspace
      ? () => setRepoSource("local")
      : setRepoSource,
    localDisabled: Boolean(selectedTag) || localSource.disabled,
    localLabel: localSource.label,
    ...repoRemote.controls,
    onCloneLocal:
      !selectedTag &&
      !isLinkedWorkspace &&
      firstCloneUrl(repository) &&
      repoRemote.canCloneLocally
        ? () => {
            void handleCloneRepo();
          }
        : undefined,
    clonePending: cloneRepoMutation.isPending,
    canPush:
      !isLinkedWorkspace &&
      !selectedTag &&
      Boolean(repoSyncStatusQuery.data?.canPush),
    onPush:
      selectedTag || isLinkedWorkspace
        ? undefined
        : () => {
            void handlePushLocalRepo();
          },
    pushDisabled:
      pushLocalRepoMutation.isPending || !repoSyncStatusQuery.data?.canPush,
    pushPending: pushLocalRepoMutation.isPending,
    pushTitle:
      repoSyncStatusQuery.data?.pushBlockReason ??
      pushPullTitle("Push", repoSyncStatusQuery.data?.aheadCount, "local"),
    canPull:
      !isLinkedWorkspace &&
      !selectedTag &&
      Boolean(repoSyncStatusQuery.data?.canPull),
    onPull:
      selectedTag || isLinkedWorkspace
        ? undefined
        : () => {
            void handlePullLocalRepo();
          },
    pullDisabled:
      pullLocalRepoMutation.isPending || !repoSyncStatusQuery.data?.canPull,
    pullPending: pullLocalRepoMutation.isPending,
    pullTitle:
      repoSyncStatusQuery.data?.pullBlockReason ??
      pushPullTitle("Pull", repoSyncStatusQuery.data?.behindCount, "remote"),
    aheadCount: repoSyncStatusQuery.data?.aheadCount ?? null,
    behindCount: repoSyncStatusQuery.data?.behindCount ?? null,
    onFetch: isLinkedWorkspace
      ? undefined
      : () => {
          void handleFetchRepo();
        },
    fetchPending:
      repoSnapshotQuery.isFetching ||
      repoStateQuery.isFetching ||
      repoSyncStatusQuery.isFetching,
    fetchTitle:
      repoSyncStatusQuery.data?.pullBlockReason ?? "Check for remote changes",
  };
  const projectPending = projectQuery.isPending;
  React.useEffect(() => {
    if (!repository) {
      // While the project query is still loading, keep the URL-seeded
      // pullRequestId/issueId selections — clearing here would discard them
      // before the detail view ever gets a chance to open.
      if (projectPending) return;
      setSelectedPullRequestId(null);
      setSelectedIssueId(null);
      setSelectedCommitHash(null);
    }
  }, [projectPending, repository]);
  React.useEffect(() => {
    setRepoSource((currentSource) => {
      if (selectedTag) return "remote";
      if (currentSource === "local" && !hasLocalCheckout) return "remote";
      if (
        currentSource === "remote" &&
        !hasRemoteSnapshot &&
        hasLocalCheckout
      ) {
        return "local";
      }
      return currentSource;
    });
  }, [hasLocalCheckout, hasRemoteSnapshot, selectedTag]);
  const peoplePubkeys = React.useMemo(() => {
    if (!repository) return [];
    // Include PR authors/updaters so commit rows can resolve avatars for
    // publishers who are not listed as project contributors.
    const pullRequestPubkeys = (pullRequestsQuery.data ?? []).flatMap(
      (pullRequest) => [
        pullRequest.author,
        ...pullRequest.updates.map((update) => update.author),
        ...pullRequest.comments.map((comment) => comment.author),
        ...pullRequest.reviewers,
        ...pullRequest.approvals.map((approval) => approval.author),
      ],
    );
    const issuePubkeys = (issuesQuery.data ?? []).flatMap((issue) => [
      issue.author,
      ...issue.recipients,
      ...issue.comments.map((comment) => comment.author),
    ]);
    return [
      ...new Set([
        ...projectPeople(repository),
        ...pullRequestPubkeys,
        ...issuePubkeys,
      ]),
    ];
  }, [issuesQuery.data, pullRequestsQuery.data, repository]);
  const profilesQuery = useUsersBatchQuery(peoplePubkeys, {
    enabled: peoplePubkeys.length > 0,
  });
  const currentProfileQuery = useProfileQuery();
  const profiles = React.useMemo(
    () =>
      mergeCurrentProfileIntoLookup(
        profilesQuery.data?.profiles,
        currentProfileQuery.data,
      ),
    [currentProfileQuery.data, profilesQuery.data?.profiles],
  );
  const identityQuery = useIdentityQuery();
  const gitIdentityQuery = useGitIdentityQuery();
  const viewerGitIdentity = React.useMemo<ViewerGitIdentity | null>(() => {
    const pubkey = identityQuery.data?.pubkey ?? null;
    if (!pubkey || !gitIdentityQuery.data) return null;
    return {
      pubkey,
      name: gitIdentityQuery.data.name,
      email: gitIdentityQuery.data.email,
    };
  }, [gitIdentityQuery.data, identityQuery.data?.pubkey]);
  const { applyPatch, values } = useHistorySearchState(
    PROJECT_DETAIL_PANEL_SEARCH_KEYS,
  );
  const profilePanelPubkey = values.profile;
  const profilePanelTab = profilePanelTabFromSearch(values.profileTab);
  const profilePanelView = profilePanelViewFromSearch(values.profileView);
  const handleOpenProfilePanel = React.useCallback(
    (pubkey: string) =>
      applyPatch({ profile: pubkey, profileTab: null, profileView: null }),
    [applyPatch],
  );
  const handleCloseProfilePanel = React.useCallback(
    () => applyPatch({ profile: null, profileTab: null, profileView: null }),
    [applyPatch],
  );
  const handleProfilePanelViewChange = React.useCallback(
    (view: ProfilePanelView, options?: { replace?: boolean }) =>
      applyPatch({ profileView: view === "summary" ? null : view }, options),
    [applyPatch],
  );
  const handleProfilePanelTabChange = React.useCallback(
    (tab: ProfilePanelTab, options?: { replace?: boolean }) =>
      applyPatch({ profileTab: tab === "info" ? null : tab }, options),
    [applyPatch],
  );
  const threadPanelWidth = useThreadPanelWidth();
  const openDmMutation = useOpenDmMutation();
  const handleOpenDm = React.useCallback(
    async (pubkeys: string[]) => {
      const dm = await openDmMutation.mutateAsync({ pubkeys });
      await goChannel(dm.id);
    },
    [goChannel, openDmMutation],
  );
  const handlePushLocalRepo = React.useCallback(async () => {
    try {
      const result = await pushLocalRepoMutation.mutateAsync();
      if (result.pullRequestUpdate.status === "failed") {
        toast.warning(result.message, {
          description: result.pullRequestUpdate.error,
        });
      } else {
        toast.success(
          result.pullRequestUpdate.status === "updated"
            ? `${result.message} Pull request updated.`
            : result.message,
        );
      }
      await Promise.all([
        repoSnapshotQuery.refetch(),
        localRepoSnapshotQuery.refetch(),
        repoSyncStatusQuery.refetch(),
        repoStateQuery.refetch(),
      ]);
    } catch (error) {
      toast.error(
        error instanceof Error ? error.message : "Failed to push repository",
      );
    }
  }, [
    localRepoSnapshotQuery,
    pushLocalRepoMutation,
    repoSnapshotQuery,
    repoStateQuery,
    repoSyncStatusQuery,
  ]);
  const handleCloneRepo = React.useCallback(async () => {
    try {
      const result = await cloneRepoMutation.mutateAsync();
      toast.success(result.message);
      setRepoSource("local");
    } catch (error) {
      showProjectCloneErrorToast(error, repository?.cloneUrls[0]);
    }
  }, [cloneRepoMutation, repository?.cloneUrls]);
  const handlePullRequestCreated = React.useCallback(
    async (
      createdProject: Project,
      createdRepository: Repository,
      pullRequestId: string,
    ) => {
      if (createdProject.id !== projectId) {
        await goProject(createdProject.id, {
          pullRequestId,
          repositoryId: createdRepository.id,
        });
        return;
      }
      if (createdRepository.id === repository?.id) {
        await pullRequestsQuery.refetch();
      } else {
        applyRepositorySearch({ repositoryId: createdRepository.id });
      }
      setSelectedPullRequestId(pullRequestId);
    },
    [
      applyRepositorySearch,
      goProject,
      projectId,
      pullRequestsQuery,
      repository?.id,
    ],
  );
  const handleCreateIssue = React.useCallback(
    async ({ body, title }: CreateIssueDialogInput) => {
      const issueId = await createIssueMutation.mutateAsync({ body, title });
      toast.success("Issue created.");
      await issuesQuery.refetch();
      setSelectedIssueId(issueId);
    },
    [createIssueMutation, issuesQuery],
  );
  const handleUpdatePullRequest = React.useCallback(async () => {
    const commit = repoSyncStatusQuery.data?.remoteHead;
    if (!commit) return;
    try {
      const updated = await updatePullRequestMutation.mutateAsync({
        commit,
        mergeBase: repoSyncStatusQuery.data?.mergeBase ?? null,
      });
      toast.success(
        updated ? "Pull request updated." : "Pull request is already current.",
      );
      await pullRequestsQuery.refetch();
    } catch (error) {
      toast.error(
        error instanceof Error
          ? error.message
          : "Failed to update pull request",
      );
    }
  }, [
    pullRequestsQuery,
    repoSyncStatusQuery.data?.mergeBase,
    repoSyncStatusQuery.data?.remoteHead,
    updatePullRequestMutation,
  ]);
  const handlePullLocalRepo = React.useCallback(async () => {
    try {
      const result = await pullLocalRepoMutation.mutateAsync();
      toast.success(result.message);
      await Promise.all([
        repoSnapshotQuery.refetch(),
        localRepoSnapshotQuery.refetch(),
        repoSyncStatusQuery.refetch(),
        repoStateQuery.refetch(),
      ]);
    } catch (error) {
      toast.error(
        error instanceof Error ? error.message : "Failed to pull repository",
      );
    }
  }, [
    localRepoSnapshotQuery,
    pullLocalRepoMutation,
    repoSnapshotQuery,
    repoStateQuery,
    repoSyncStatusQuery,
  ]);
  const openTerminal = useOpenProjectTerminal(activeCommunity?.reposDir);
  const handleOpenTerminal = React.useCallback(() => {
    if (!repository) return Promise.resolve();
    return openTerminal(repository, {
      branch: activeBranch,
      hasLocalCheckout,
    });
  }, [activeBranch, hasLocalCheckout, openTerminal, repository]);
  const handleOpenMergeRecoveryTerminal = React.useCallback(
    async (input: {
      expectedCommit: string;
      sourceBranch: string;
      sourceCloneUrl: string;
      targetBranch: string;
    }) => {
      const targetCloneUrl = firstCloneUrl(repository);
      if (!repository || !targetCloneUrl || isLinkedWorkspace) {
        throw new Error("No mutable managed checkout is available.");
      }
      return openProjectMergeRecoveryTerminal({
        ...input,
        projectDtag: repository.dtag,
        reposDir: activeCommunity?.reposDir,
        targetCloneUrl,
      });
    },
    [activeCommunity?.reposDir, isLinkedWorkspace, repository],
  );

  if (projectQuery.isLoading) {
    return <ProjectDetailScreenLoading />;
  }
  if (projectQuery.isError) {
    return (
      <ProjectDetailScreenError
        onGoProjects={() => {
          void goProjects();
        }}
        onRetry={() => {
          void projectQuery.refetch();
        }}
      />
    );
  }
  if (!project) {
    return (
      <ProjectDetailScreenNotFound
        onGoProjects={() => {
          void goProjects();
        }}
      />
    );
  }
  if (!repository) {
    return (
      <ProjectOutcomeDetail project={project} pullRequests={[]}>
        <ProjectDetailScreenNoRepository project={project} />
      </ProjectOutcomeDetail>
    );
  }

  const repoContributors = repoSnapshotQuery.data?.contributors ?? [];
  const selectedPullRequest =
    pullRequestsQuery.data?.find((item) => item.id === selectedPullRequestId) ??
    null;
  const selectedIssue =
    issuesQuery.data?.find((item) => item.id === selectedIssueId) ?? null;
  const displayedSnapshotCommits =
    effectiveRepoSource === "local"
      ? (localRepoSnapshotQuery.data?.snapshot.commits ?? [])
      : (repoSnapshotQuery.data?.commits ?? []);
  const activeWorkItemCrumb = buildProjectDetailWorkItemCrumb({
    selectedCommitHash,
    selectedIssue,
    selectedPullRequest,
    snapshotCommits: displayedSnapshotCommits,
    setSelectedCommitHash,
    setSelectedIssueId,
    setSelectedPullRequestId,
  });
  // Sub-tab crumb when no work item is open. Overview (readme) is home.
  const activeTabCrumb = activeWorkItemCrumb
    ? null
    : (PROJECT_TAB_CRUMB_LABELS[activeTab] ?? null);
  const handleGoToProjectHome = () => {
    setSelectedPullRequestId(null);
    setSelectedIssueId(null);
    setSelectedCommitHash(null);
    // Remount the workspace tabs so the project page opens on Overview
    // instead of whatever tab the work item left behind.
    setTabsResetKey((key) => key + 1);
  };
  const handleRepositoryChange = (nextRepositoryId: string) => {
    applyRepositorySearch({
      repositoryId: nextRepositoryId,
      issueId: null,
      pullRequestId: null,
      commitHash: null,
    });
    setSelectedPullRequestId(null);
    setSelectedIssueId(null);
    setSelectedCommitHash(null);
    setRepoSource("remote");
    setTabsResetKey((key) => key + 1);
  };

  return (
    <ProfilePanelProvider onOpenProfilePanel={handleOpenProfilePanel}>
      <ProjectBranchActionDialogs
        actions={branchActions}
        activeBranch={activeBranch}
        activeBranchCommit={activeBranchCommit}
        existingBranches={branchOptionsWithLocal}
      />
      <div className="flex min-h-0 min-w-0 flex-1 flex-row overflow-hidden">
        <div className="relative flex min-h-0 min-w-0 flex-1 flex-col overflow-hidden">
          <ProjectDetailChrome
            activeTabCrumb={activeTabCrumb}
            activeWorkItemCrumb={activeWorkItemCrumb}
            chromeRef={projectDetailHeaderChromeRef}
            onGoChannel={(channelId) => {
              void goChannel(channelId);
            }}
            onGoProjectHome={handleGoToProjectHome}
            onGoProjects={() => {
              void goProjects();
            }}
            project={project}
          />

          <div className="flex min-h-0 min-w-0 flex-1 flex-col overflow-y-auto px-4 pb-4">
            <div className="w-full space-y-3 pt-[calc(var(--buzz-channel-content-top-padding,5.75rem)_+_1px)]">
              <ProjectDetailRepositoryHeader
                identityPubkey={identityQuery.data?.pubkey}
                onRepositoryChange={handleRepositoryChange}
                project={project}
                projects={projectsQuery.data ?? []}
                repoRemote={repoRemote}
                repoSource={repoSource}
                repository={repository}
              />

              <ProjectOutcomeDetail
                profiles={profiles}
                project={project}
                pullRequests={
                  projectWorkItemsQuery.data?.pullRequests.items.map(
                    ({ pullRequest }) => pullRequest,
                  ) ?? []
                }
              >
                <WorkspaceTabs
                  key={`${project.id}:${repository.id}:${tabsResetKey}`}
                  commitDiff={commitDiffQuery.data}
                  commitDiffError={commitDiffQuery.error}
                  commitDiffLoading={commitDiffQuery.isLoading}
                  createIssueAction={{
                    onCreate: handleCreateIssue,
                    pending: createIssueMutation.isPending,
                  }}
                  createPullRequestAction={
                    isLinkedWorkspace
                      ? undefined
                      : {
                          onCreated: handlePullRequestCreated,
                          projects: projectsQuery.data ?? [project],
                          reposDir: activeCommunity?.reposDir,
                        }
                  }
                  updatePullRequestAction={
                    !isLinkedWorkspace &&
                    openBranchPullRequest &&
                    repoSyncStatusQuery.data?.remoteHead &&
                    repoSyncStatusQuery.data.remoteHead !==
                      openBranchPullRequest.commit
                      ? {
                          onUpdate: () => {
                            void handleUpdatePullRequest();
                          },
                          pending: updatePullRequestMutation.isPending,
                        }
                      : undefined
                  }
                  localSnapshot={localRepoSnapshotQuery.data}
                  localSnapshotError={localRepoSnapshotQuery.error}
                  localSnapshotLoading={localRepoSnapshotQuery.isLoading}
                  onBranchChange={handleBranchChange}
                  onOpenMergeRecoveryTerminal={
                    isLinkedWorkspace
                      ? undefined
                      : handleOpenMergeRecoveryTerminal
                  }
                  onOpenTerminal={
                    canOpenTerminal
                      ? () => void handleOpenTerminal()
                      : undefined
                  }
                  terminalTitle={projectTerminalLabel(hasLocalCheckout)}
                  onSelectedCommitHashChange={handleSelectedCommitHashChange}
                  onSelectedIssueIdChange={handleSelectedIssueIdChange}
                  onSelectedPullRequestIdChange={
                    handleSelectedPullRequestIdChange
                  }
                  onSelectedTabChange={setActiveTab}
                  profiles={profiles}
                  project={repository}
                  projectId={project.id}
                  repoDiff={displayedRepoDiff}
                  repoDiffError={displayedRepoDiffError}
                  repoDiffLoading={displayedRepoDiffLoading}
                  pullRequests={pullRequestsQuery.data ?? []}
                  pullRequestsError={pullRequestsQuery.error}
                  pullRequestsLoading={pullRequestsQuery.isLoading}
                  repoContributors={repoContributors}
                  repoHost={repoRemote.host}
                  repoSource={effectiveRepoSource}
                  selectedCommitHash={selectedCommitHash}
                  selectedIssueId={selectedIssueId}
                  selectedPullRequestId={selectedPullRequestId}
                  snapshot={repoSnapshotQuery.data}
                  snapshotError={repoSnapshotQuery.error}
                  snapshotLoading={repoSnapshotQuery.isLoading}
                  sourceControls={filesSourceControls}
                  viewerGitIdentity={viewerGitIdentity}
                />
              </ProjectOutcomeDetail>
            </div>
          </div>
        </div>
        {profilePanelPubkey ? (
          <UserProfilePanel
            canResetWidth={threadPanelWidth.canReset}
            currentPubkey={identityQuery.data?.pubkey}
            onClose={handleCloseProfilePanel}
            onOpenDm={handleOpenDm}
            onOpenProfile={handleOpenProfilePanel}
            onResetWidth={threadPanelWidth.onResetWidth}
            onResizeStart={threadPanelWidth.onResizeStart}
            onTabChange={handleProfilePanelTabChange}
            onViewChange={handleProfilePanelViewChange}
            pubkey={profilePanelPubkey}
            tab={profilePanelTab}
            view={profilePanelView}
            widthPx={threadPanelWidth.widthPx}
          />
        ) : null}
      </div>
    </ProfilePanelProvider>
  );
}

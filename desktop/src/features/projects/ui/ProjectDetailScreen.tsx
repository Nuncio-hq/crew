import { firstCloneUrl } from "@/features/projects/lib/projectCloneUrl";
import { ProjectDetailRepositoryHeader } from "./project-detail-navigation";
import { Button } from "@/shared/ui/button";
import { useThreadPanelWidth } from "@/shared/hooks/useThreadPanelWidth";
import { localWorkspaceSourceState } from "@/features/projects/lib/project-exact-local-workspace";
import * as React from "react";
import { toast } from "sonner";

import { useAppNavigation } from "@/app/navigation/useAppNavigation";
import { useTerminalContextOverride } from "@/app/TerminalContextOverrideContext";
import {
  type Project,
  type Repository,
  useProjectQuery,
  useProjectIssuesQuery,
  useProjectPullRequestsQuery,
  useProjectsQuery,
  useProjectsWorkItemsQuery,
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
import { UserProfilePanel } from "@/features/profile/ui/UserProfilePanel";
import { ProfilePanelProvider } from "@/shared/context/ProfilePanelContext";
import { useHistorySearchState } from "@/shared/hooks/useHistorySearchState";
import { ViewLoadingFallback } from "@/shared/ui/ViewLoadingFallback";
import { useCommunities } from "@/features/communities/useCommunities";
import {
  projectBranchCreationReason,
  projectBranchManagementState,
  projectBranchOptionsFromSync,
  resolveProjectDefaultBranch,
} from "@/features/projects/lib/projectBranches";
import {
  projectRepoUnavailablePresentation,
  projectRepoUnavailableReason,
  refineRepoUnavailableReason,
} from "@/features/projects/lib/projectRepoAvailability";
import { selectProjectRepository } from "@/features/projects/projectModels";
import { useMemberChannelIds } from "@/features/projects/useRepositoryAccess";
import { KIND_REPO_ANNOUNCEMENT } from "@/shared/constants/kinds";
import { useProjectRepoPresentation } from "@/features/projects/useProjectRepoHost";
import { WorkspaceTabs } from "./ProjectWorkspaceTabs";
import { ProjectOutcomeDetail } from "./ProjectOutcomeDetail";
import type { RepoSourceHeaderControls } from "./ProjectRepositorySource";
import { showProjectCloneErrorToast } from "./projectGitErrorToast";
import { projectTerminalLabel } from "./useOpenProjectTerminal";
import type { CreateIssueDialogInput } from "./CreateIssueDialog";
import { ProjectBranchActionDialogs } from "./ProjectBranchActionDialogs";
import { ProjectDetailChrome } from "./ProjectDetailChrome";
import { ProjectDetailUnavailableState } from "./ProjectDetailUnavailableState";
import { buildProjectDetailCrumbs } from "./useProjectDetailCrumbs";
import { useProjectDetailPeople } from "./useProjectDetailPeople";
import { useProjectProfilePanel } from "./useProjectProfilePanel";
import { useRepositoryFileContentSource } from "./useRepositoryFileContentSource";
import { useProjectRepositoryOpenActions } from "./useProjectRepositoryOpenActions";
import {
  useProjectDetailGitViews,
  useRetainedPullRequestSelection,
} from "./useRetainedProjectGitViews";
import {
  PROJECT_REPOSITORY_SEARCH_KEYS,
  type ProjectDetailScreenProps,
  pushPullTitle,
  snapshotHasContent,
} from "./projectDetailHelpers";

export function ProjectDetailScreen(props: ProjectDetailScreenProps) {
  const {
    commitHash,
    entityNavigationId,
    filePath,
    projectId,
    pullRequestId,
    issueId,
    repositoryId,
    tab,
  } = props;
  const { goChannel, goProject, goProjects } = useAppNavigation();
  const { activeCommunity } = useCommunities();
  const projectQuery = useProjectQuery(projectId);
  const projectsQuery = useProjectsQuery();
  const project = projectQuery.data;
  const projectWorkItemsQuery = useProjectsWorkItemsQuery(
    project ? [project] : [],
  );
  const routeRepositoryId: string | undefined = React.useMemo(() => {
    if (repositoryId) return repositoryId;
    const kindStr = `${String(KIND_REPO_ANNOUNCEMENT)}:`;
    if (!projectId.startsWith(kindStr)) return undefined;
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
      repositoryId: repository?.id ?? null,
      tags: repoStateQuery.data?.tags ?? [],
    });
  const activeTag =
    repoStateQuery.data?.tags.find((tag) => tag.name === selectedTag) ?? null;
  const [selectedPullRequestId, setSelectedPullRequestId] = React.useState<
    string | null
  >(pullRequestId ?? null);
  const [selectedIssueId, setSelectedIssueId] = React.useState<string | null>(
    issueId ?? null,
  );
  const createIssueRequestKey = 0;
  const createPullRequestRequestKey = 0;
  // biome-ignore lint/correctness/useExhaustiveDependencies: the transient request ID deliberately reapplies an unchanged entity selection.
  React.useEffect(() => {
    setSelectedPullRequestId(pullRequestId ?? null);
    setSelectedIssueId(issueId ?? null);
  }, [entityNavigationId, issueId, pullRequestId]);
  const [selectedCommitHash, setSelectedCommitHash] = React.useState<
    string | null
  >(commitHash ?? null);
  React.useEffect(
    () => setSelectedCommitHash(commitHash ?? null),
    [commitHash],
  );
  const [tabsResetKey, setTabsResetKey] = React.useState(0);
  const [requestedTab, setRequestedTab] = React.useState<string | undefined>(
    tab,
  );
  // biome-ignore lint/correctness/useExhaustiveDependencies: the transient request ID deliberately reapplies an unchanged share-link tab.
  React.useEffect(() => setRequestedTab(tab), [entityNavigationId, tab]);
  const [activeTab, setActiveTab] = React.useState("overview");
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
  const {
    activeRepoPullRequest,
    openBranchPullRequest,
    selectedBranchPullRequest,
    selectedPullRequest,
  } = useRetainedPullRequestSelection({
    activeBranch,
    isFetching: pullRequestsQuery.isFetching,
    pullRequests: pullRequestsQuery.data,
    repository,
    selectedPullRequestId,
  });
  const [repoSource, setRepoSource] = React.useState<"remote" | "local">(
    "remote",
  );
  const effectiveRepoSource = isLinkedWorkspace ? "local" : repoSource;
  const {
    commitDiffQuery,
    displayedRepoDiff,
    displayedRepoDiffError,
    displayedRepoDiffLoading,
    displayedRepoSnapshot,
    localRepoSnapshotQuery,
    repoSnapshotQuery,
  } = useProjectDetailGitViews({
    activeBranch,
    activeRepoPullRequest,
    activeTag,
    isBuzzHost: repoRemote.host.kind === "buzz",
    repository,
    reposDir: activeCommunity?.reposDir,
    repoSource: effectiveRepoSource,
    selectedBranchPullRequest,
    selectedCommitHash,
    selectedTag,
  });
  const memberChannelIds = useMemberChannelIds();
  const remoteUnavailableReason =
    repoRemote.host.kind === "buzz" &&
    !repoSnapshotQuery.isLoading &&
    !displayedRepoSnapshot
      ? refineRepoUnavailableReason({
          reason: projectRepoUnavailableReason(repoSnapshotQuery.error),
          repositoryChannelId: repository?.channelId,
          memberChannelIds,
        })
      : undefined;
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
      snapshotCommit: displayedRepoSnapshot?.latestCommit?.hash,
    });
  const handleBranchChange = React.useCallback(
    (branch: string | null) => {
      selectBranch(branch);
      if (!branch) return;
      const localBranches = repoSyncStatusQuery.data?.localBranches;
      if (
        !isLinkedWorkspace &&
        effectiveRepoSource === "local" &&
        localBranches &&
        !localBranches.includes(branch)
      ) {
        setRepoSource("remote");
      }
    },
    [
      effectiveRepoSource,
      isLinkedWorkspace,
      repoSyncStatusQuery.data?.localBranches,
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
      const reason = refineRepoUnavailableReason({
        reason: projectRepoUnavailableReason(error),
        repositoryChannelId: repository?.channelId,
        memberChannelIds,
      });
      const presentation = projectRepoUnavailablePresentation(reason);
      toast.error(presentation.title, {
        description: presentation.description,
      });
      return;
    }
    toast.success("Remote state refreshed.");
  }, [
    memberChannelIds,
    repoSnapshotQuery,
    repoStateQuery,
    repoSyncStatusQuery,
    repository?.channelId,
  ]);
  const cloneBlockedByRemote =
    remoteUnavailableReason !== undefined &&
    remoteUnavailableReason !== "ref" &&
    remoteUnavailableReason !== "unknown";
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
    localPath:
      repoSyncStatusQuery.data?.localPath ?? localRepoSnapshotQuery.data?.path,
    ...repoRemote.controls,
    remoteUnavailableReason,
    onAskForAccess: () => {
      if (repository?.channelId) void goChannel(repository.channelId);
    },
    onCloneLocal:
      !selectedTag &&
      !isLinkedWorkspace &&
      !cloneBlockedByRemote &&
      repository?.cloneUrls[0] &&
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
  const fileContentSource = useRepositoryFileContentSource({
    activeBranch,
    activeTag,
    pullRequest: selectedBranchPullRequest,
    repository,
    reposDir: activeCommunity?.reposDir,
    selectedTag,
    source: effectiveRepoSource,
  });
  const projectPending = projectQuery.isPending;
  React.useEffect(() => {
    if (!repository) {
      if (projectPending) return;
      setSelectedPullRequestId(null);
      setSelectedIssueId(null);
      setSelectedCommitHash(null);
    }
  }, [projectPending, repository]);
  React.useEffect(() => {
    if (selectedTag) {
      if (repoSource !== "remote") setRepoSource("remote");
      return;
    }
    if (repoSource === "local" && !hasLocalCheckout) {
      setRepoSource("remote");
      return;
    }
    if (
      !selectedPullRequestId &&
      repoSource === "remote" &&
      !snapshotHasContent(displayedRepoSnapshot) &&
      hasLocalCheckout
    ) {
      setRepoSource("local");
    }
  }, [
    displayedRepoSnapshot,
    hasLocalCheckout,
    repoSource,
    selectedPullRequestId,
    selectedTag,
  ]);
  const {
    contributorActivityCounts,
    contributorPubkeys,
    identityPubkey,
    profiles,
    viewerGitIdentity,
  } = useProjectDetailPeople({
    issues: issuesQuery.data ?? [],
    pullRequests: pullRequestsQuery.data ?? [],
    repository,
  });
  const {
    handleCloseProfilePanel,
    handleOpenDm,
    handleOpenProfilePanel,
    handleProfilePanelTabChange,
    handleProfilePanelViewChange,
    profilePanelPubkey,
    profilePanelTab,
    profilePanelView,
  } = useProjectProfilePanel();
  const threadPanelWidth = useThreadPanelWidth();
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
            ? `${result.message} Review updated.`
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
      const unavailableReason = refineRepoUnavailableReason({
        reason: projectRepoUnavailableReason(error),
        repositoryChannelId: repository?.channelId,
        memberChannelIds,
      });
      showProjectCloneErrorToast(
        error,
        repository?.cloneUrls[0],
        unavailableReason,
      );
    }
  }, [
    cloneRepoMutation,
    memberChannelIds,
    repository?.channelId,
    repository?.cloneUrls,
  ]);
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
    async (input: CreateIssueDialogInput) => {
      const issueId = await createIssueMutation.mutateAsync(input);
      toast.success("Task created.");
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
      toast.success(updated ? "Review updated." : "Review is already current.");
      await pullRequestsQuery.refetch();
    } catch (error) {
      toast.error(
        error instanceof Error ? error.message : "Failed to update review",
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
  const { handleOpenMergeRecoveryTerminal, handleOpenTerminal } =
    useProjectRepositoryOpenActions({
      activeBranch,
      hasLocalCheckout,
      localRepositoryPath: filesSourceControls.localPath ?? null,
      repository,
      reposDir: activeCommunity?.reposDir,
    });
  const projectTerminalContext = React.useMemo(() => {
    const channelId = repository?.channelId ?? project?.projectChannelId;
    if (!channelId) return null;
    return {
      channelId,
      channelName: repository?.name ?? project?.name ?? "Project",
    };
  }, [
    project?.name,
    project?.projectChannelId,
    repository?.channelId,
    repository?.name,
  ]);
  useTerminalContextOverride(projectTerminalContext);
  if (projectQuery.isLoading) {
    return <ViewLoadingFallback kind="projects" />;
  }
  if (projectQuery.isError) {
    return (
      <ProjectDetailUnavailableState
        kind="load-error"
        onBack={() => void goProjects()}
        onRetry={() => void projectQuery.refetch()}
      />
    );
  }
  if (!project) {
    return (
      <ProjectDetailUnavailableState
        kind="not-found"
        onBack={() => void goProjects()}
      />
    );
  }
  if (!repository) {
    return (
      <ProjectDetailUnavailableState
        kind="repositories-unavailable"
        project={project}
      />
    );
  }
  const repoContributors = displayedRepoSnapshot?.contributors ?? [];
  const selectedIssue =
    issuesQuery.data?.find((item) => item.id === selectedIssueId) ?? null;
  const displayedSnapshotCommits =
    effectiveRepoSource === "local"
      ? (localRepoSnapshotQuery.data?.snapshot.commits ?? [])
      : (displayedRepoSnapshot?.commits ?? []);
  const selectedCommit = selectedCommitHash
    ? (displayedSnapshotCommits.find(
        (commit) => commit.hash === selectedCommitHash,
      ) ?? null)
    : null;
  const { activeTabCrumb, activeWorkItemCrumb, handleGoToProjectHome } =
    buildProjectDetailCrumbs({
      activeTab,
      commit: selectedCommit,
      issue: selectedIssue,
      pullRequest: selectedPullRequest,
      setRequestedTab,
      setSelectedCommitHash,
      setSelectedIssueId,
      setSelectedPullRequestId,
      setTabsResetKey,
    });
  const goChannelHome = () => {
    if (project.projectChannelId) {
      void goProject(project.id);
      return;
    }
    handleGoToProjectHome();
  };
  const sharedHeaderBackdrop =
    !selectedPullRequestId && !selectedIssueId && !selectedCommitHash;
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
    setRequestedTab(undefined);
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
        <div className="relative flex min-h-0 min-w-0 flex-1 flex-col overflow-hidden [container-type:inline-size]">
          <ProjectDetailChrome
            activeTabCrumb={activeTabCrumb}
            activeWorkItemCrumb={activeWorkItemCrumb}
            onGoProjectHome={goChannelHome}
            onGoProjects={() => {
              void goProjects();
            }}
            project={project}
            repository={repository}
          />
          <div
            className="flex min-h-0 min-w-0 flex-1 flex-col overflow-x-hidden overflow-y-auto overscroll-y-none px-4 pb-4"
            data-testid="project-detail-scroll"
          >
            {/* min-h-full + flex chain lets the commit detail's diff pane
                    grow to the bottom of the scrollport without forcing a
                    taller page when content already overflows. */}
            <div className="flex min-h-full w-full flex-col space-y-3">
              <ProjectDetailRepositoryHeader
                identityPubkey={identityPubkey}
                onRepositoryChange={handleRepositoryChange}
                project={project}
                projects={projectsQuery.data ?? []}
                repoRemote={repoRemote}
                repoSource={effectiveRepoSource}
                repository={repository}
              />
              {canOpenTerminal ? (
                <Button
                  className="self-end"
                  onClick={() => void handleOpenTerminal()}
                  size="sm"
                  variant="outline"
                >
                  {projectTerminalLabel(hasLocalCheckout)}
                </Button>
              ) : null}
              <ProjectOutcomeDetail
                openPlumbing={Boolean(
                  tab || commitHash || issueId || pullRequestId || filePath,
                )}
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
                  initialTab={
                    requestedTab
                      ? requestedTab === "commits"
                        ? "activity"
                        : requestedTab
                      : undefined
                  }
                  initialFilePath={filePath}
                  initialTabRequestKey={entityNavigationId}
                  fileContentSource={fileContentSource}
                  commitDiff={commitDiffQuery.data}
                  commitDiffError={commitDiffQuery.error}
                  commitDiffLoading={commitDiffQuery.isLoading}
                  contributorActivityCounts={contributorActivityCounts}
                  contributorPubkeys={contributorPubkeys}
                  createIssueAction={{
                    onCreate: handleCreateIssue,
                    pending: createIssueMutation.isPending,
                  }}
                  createIssueRequestKey={createIssueRequestKey}
                  createPullRequestAction={
                    isLinkedWorkspace
                      ? undefined
                      : {
                          onCreated: handlePullRequestCreated,
                          projects: projectsQuery.data ?? [project],
                          reposDir: activeCommunity?.reposDir,
                        }
                  }
                  createPullRequestRequestKey={createPullRequestRequestKey}
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
                  onOpenMergeRecoveryTerminal={
                    isLinkedWorkspace
                      ? undefined
                      : handleOpenMergeRecoveryTerminal
                  }
                  onSelectedCommitHashChange={handleSelectedCommitHashChange}
                  onSelectedIssueIdChange={handleSelectedIssueIdChange}
                  onSelectedPullRequestIdChange={
                    handleSelectedPullRequestIdChange
                  }
                  onSelectedTabChange={setActiveTab}
                  onBack={goChannelHome}
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
                  selectedPullRequest={selectedPullRequest}
                  selectedPullRequestId={selectedPullRequestId}
                  sharedHeaderBackdrop={sharedHeaderBackdrop}
                  snapshot={displayedRepoSnapshot}
                  snapshotError={repoSnapshotQuery.error}
                  snapshotLoading={
                    repoSnapshotQuery.isLoading && !displayedRepoSnapshot
                  }
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
            currentPubkey={identityPubkey}
            onClose={handleCloseProfilePanel}
            onOpenDm={handleOpenDm}
            onOpenProfile={handleOpenProfilePanel}
            onResetWidth={threadPanelWidth.onResetWidth}
            onResizeStart={threadPanelWidth.onResizeStart}
            onTabChange={handleProfilePanelTabChange}
            onViewChange={handleProfilePanelViewChange}
            pubkey={profilePanelPubkey}
            tab={profilePanelTab}
            transparentChrome={sharedHeaderBackdrop}
            view={profilePanelView}
            widthPx={threadPanelWidth.widthPx}
          />
        ) : null}
      </div>
    </ProfilePanelProvider>
  );
}

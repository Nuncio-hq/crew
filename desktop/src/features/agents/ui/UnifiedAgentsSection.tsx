import {
  deriveNoChannelMembershipBadge,
  useNoChannelMembership,
} from "../lib/channelMembershipState";
import { AgentChannelMembershipBadge } from "./AgentChannelMembershipBadge";
import * as React from "react";
import { AlertTriangle, ChevronDown, ChevronRight, Moon } from "lucide-react";

import {
  isAgentCardAvatarLoading,
  resolveAgentCardAvatarUrl,
} from "@/features/agents/lib/agentCardAvatar";
import { useAgentCardModelLabel } from "@/features/agents/ui/AgentCardModelLabel";
import { effectiveAgentDescription } from "@/features/agents/lib/agentDescription";
import { friendlyAgentLastError } from "@/features/agents/lib/friendlyAgentLastError";
import type { AgentAvailabilityReader } from "@/features/agents/lib/useAgentAvailability";
import { isManagedAgentActive } from "@/features/agents/lib/managedAgentControlActions";
import { useManagedAgentRuntimesQuery } from "@/features/agents/managedAgentRuntimeHooks";
import {
  findManagedAgentRuntime,
  MANAGED_AGENT_SLEEPING_BADGE_LABEL,
  shouldShowManagedAgentSleepingBadge,
} from "@/features/agents/managedAgentRuntimeStatus";
import { useCommunities } from "@/features/communities/useCommunities";
import { useIsArchivedPredicate } from "@/features/identity-archive/hooks";
import { useUserProfileQuery } from "@/features/profile/hooks";
import type {
  AgentPersona,
  ManagedAgent,
  ManagedAgentRuntimeStatus,
} from "@/shared/api/types";
import type { ProfilePanelOpenOptions } from "@/shared/context/ProfilePanelContext";
import { useFeedbackToasts } from "@/shared/hooks/useToastEffect";
import { Badge } from "@/shared/ui/badge";
import {
  ProtectedBestieCardBadge,
  useProtectedBestiePubkey,
} from "@protected-feature-components";
import { IdentityCardSkeleton } from "@/shared/ui/identity-card-skeleton";
import { AgentIdentityCard } from "./AgentIdentityCard";
import { AgentRuntimeAvatarControl } from "./AgentRuntimeAvatarControl";
import { resolveAgentDefaultRuntimeId } from "./AgentRuntimeDefaultAvatar";
import { CreateIdentityCard } from "./CreateIdentityCard";
import { HermesProfileReadinessIndicator } from "./HermesProfileReadinessIndicator";
import { PersonaActionsMenu } from "./PersonaActionsMenu";
import { buildUnifiedGroups, pickProfileAgent } from "./unifiedAgentGroups";

type UnifiedAgentsSectionProps = {
  defaultModel: string;
  getAvailability: AgentAvailabilityReader;
  actionErrorMessage: string | null;
  actionNoticeMessage: string | null;
  agents: ManagedAgent[];
  agentsError: Error | null;
  isActionPending: boolean;
  isAgentsLoading: boolean;
  restartingAgentPubkey: string | null;
  startingAgentPubkey: string | null;
  startingPersonaIds: ReadonlySet<string>;
  onOpenAgentProfile: (
    pubkey: string,
    options?: ProfilePanelOpenOptions,
  ) => void;
  onOpenPersonaProfile: (persona: AgentPersona) => void;
  onRestartAgent: (pubkey: string) => void;
  onStartAgent: (pubkey: string) => void;
  onStartPersona: (persona: AgentPersona) => void;
  personas: AgentPersona[];
  personasError: Error | null;
  personaFeedbackErrorMessage: string | null;
  personaFeedbackNoticeMessage: string | null;
  isPersonasLoading: boolean;
  isPersonasPending: boolean;
  onOpenCatalog: () => void;
  onDuplicatePersona: (persona: AgentPersona) => void;
  onEditPersona: (persona: AgentPersona, linkedAgent?: ManagedAgent) => void;
  onSharePersona: (
    persona: AgentPersona,
    linkedAgent: ManagedAgent | undefined,
    effectiveAvatarUrl: string | null,
  ) => void;
  onDeactivatePersona: (persona: AgentPersona) => void;
  onDeletePersona: (persona: AgentPersona) => void;
};

const AGENT_CARD_COLUMN_CLASS = "w-full";
export const AGENT_CARD_GRID_COLUMNS_CLASS =
  "grid-cols-1 [@container(min-width:21rem)]:grid-cols-2 [@container(min-width:32rem)]:grid-cols-3 [@container(min-width:43rem)]:grid-cols-4 [@container(min-width:54rem)]:grid-cols-5";
export const IDENTITY_CARD_GRID_CLASS = `${AGENT_CARD_COLUMN_CLASS} ${AGENT_CARD_GRID_COLUMNS_CLASS} grid gap-3`;

export function UnifiedAgentsSection(props: UnifiedAgentsSectionProps) {
  const {
    actionErrorMessage,
    actionNoticeMessage,
    defaultModel,
    getAvailability,
    agents,
    agentsError,
    isActionPending,
    isAgentsLoading,
    restartingAgentPubkey,
    startingAgentPubkey,
    startingPersonaIds,
    onOpenAgentProfile,
    onOpenPersonaProfile,
    onRestartAgent,
    onStartAgent,
    onStartPersona,
    personas,
    personasError,
    personaFeedbackErrorMessage,
    personaFeedbackNoticeMessage,
    isPersonasLoading,
    isPersonasPending,
    onOpenCatalog,
    onDuplicatePersona,
    onEditPersona,
    onSharePersona,
    onDeactivatePersona,
    onDeletePersona,
  } = props;

  const isArchived = useIsArchivedPredicate();
  const bestiePubkey = useProtectedBestiePubkey(agents)?.toLowerCase() ?? null;
  const { groups, ungrouped, unknown } = React.useMemo(
    () => buildUnifiedGroups(personas, agents, isArchived),
    [personas, agents, isArchived],
  );
  const { activeCommunity } = useCommunities();
  const activeRelayUrl = activeCommunity?.relayUrl ?? null;
  const managedAgentRuntimesQuery = useManagedAgentRuntimesQuery({
    enabled: Boolean(activeRelayUrl && agents.length > 0),
  });
  const runtimeByPubkey = React.useMemo(() => {
    const runtimes = new Map<string, ManagedAgentRuntimeStatus>();
    if (!activeRelayUrl) return runtimes;
    for (const agent of agents) {
      const runtime = findManagedAgentRuntime(
        managedAgentRuntimesQuery.data ?? [],
        agent.pubkey,
        activeRelayUrl,
      );
      if (runtime) runtimes.set(agent.pubkey.toLowerCase(), runtime);
    }
    return runtimes;
  }, [activeRelayUrl, agents, managedAgentRuntimesQuery.data]);
  const [collapsed, setCollapsed] = React.useState<Set<string>>(new Set());
  function toggle(key: string) {
    setCollapsed((prev) => {
      const next = new Set(prev);
      if (next.has(key)) next.delete(key);
      else next.add(key);
      return next;
    });
  }

  useFeedbackToasts(actionNoticeMessage, actionErrorMessage);
  useFeedbackToasts(personaFeedbackNoticeMessage, personaFeedbackErrorMessage);
  const isLoading = isAgentsLoading || isPersonasLoading;

  return (
    <section
      className="relative space-y-4"
      data-testid="agents-library-personas"
    >
      {isLoading ? <LoadingSkeleton /> : null}

      {!isLoading ? (
        <div className="space-y-3" data-testid="unified-agents-groups">
          <div className={IDENTITY_CARD_GRID_CLASS}>
            <CreateIdentityCard
              ariaLabel="New agent"
              dataTestId="new-agent-card"
              disabled={isPersonasPending}
              onClick={onOpenCatalog}
            />
            {groups.map((group) => {
              const profileAgent = pickProfileAgent(group.agents, isArchived);
              return (
                <AgentPersonaCard
                  actions={(effectiveAvatarUrl, isEffectiveAvatarLoading) => (
                    <PersonaActionsMenu
                      isActionPending={
                        isActionPending || isEffectiveAvatarLoading
                      }
                      isPending={isPersonasPending}
                      persona={group.persona}
                      linkedAgent={profileAgent}
                      onDeactivate={onDeactivatePersona}
                      onDelete={onDeletePersona}
                      onDuplicate={onDuplicatePersona}
                      onEdit={onEditPersona}
                      onShare={(persona, linkedAgent) =>
                        onSharePersona(persona, linkedAgent, effectiveAvatarUrl)
                      }
                    />
                  )}
                  agent={profileAgent}
                  getAvailability={getAvailability}
                  defaultModel={defaultModel}
                  isBestie={profileAgent?.pubkey.toLowerCase() === bestiePubkey}
                  key={group.persona.id}
                  managedAgentRuntime={
                    profileAgent
                      ? runtimeByPubkey.get(profileAgent.pubkey.toLowerCase())
                      : undefined
                  }
                  persona={group.persona}
                  restartingAgentPubkey={restartingAgentPubkey}
                  startingAgentPubkey={startingAgentPubkey}
                  startingPersonaIds={startingPersonaIds}
                  onOpenAgentProfile={onOpenAgentProfile}
                  onOpenPersonaProfile={onOpenPersonaProfile}
                  onRestartAgent={onRestartAgent}
                  onStartAgent={onStartAgent}
                  onStartPersona={onStartPersona}
                />
              );
            })}
          </div>

          {unknown.length > 0 ? (
            <CollapsibleAgentGroup
              agents={unknown}
              collapsed={collapsed}
              getAvailability={getAvailability}
              defaultModel={defaultModel}
              groupKey="__unknown__"
              bestiePubkey={bestiePubkey}
              label="Unknown agents"
              restartingAgentPubkey={restartingAgentPubkey}
              runtimeByPubkey={runtimeByPubkey}
              startingAgentPubkey={startingAgentPubkey}
              onToggle={toggle}
              onOpenAgentProfile={onOpenAgentProfile}
              onRestartAgent={onRestartAgent}
              onStartAgent={onStartAgent}
            />
          ) : null}
          {ungrouped.length > 0 ? (
            <CollapsibleAgentGroup
              agents={ungrouped}
              collapsed={collapsed}
              getAvailability={getAvailability}
              defaultModel={defaultModel}
              groupKey="__ungrouped__"
              bestiePubkey={bestiePubkey}
              label="Custom agents"
              restartingAgentPubkey={restartingAgentPubkey}
              runtimeByPubkey={runtimeByPubkey}
              startingAgentPubkey={startingAgentPubkey}
              onToggle={toggle}
              onOpenAgentProfile={onOpenAgentProfile}
              onRestartAgent={onRestartAgent}
              onStartAgent={onStartAgent}
            />
          ) : null}
        </div>
      ) : null}

      {agentsError ? (
        <p
          className={`${AGENT_CARD_COLUMN_CLASS} rounded-2xl border border-destructive/30 bg-destructive/10 px-4 py-3 text-sm text-destructive`}
        >
          {agentsError.message}
        </p>
      ) : null}
      {personasError ? (
        <p
          className={`${AGENT_CARD_COLUMN_CLASS} rounded-2xl border border-destructive/30 bg-destructive/10 px-4 py-3 text-sm text-destructive`}
        >
          {personasError.message}
        </p>
      ) : null}
    </section>
  );
}

function AgentPersonaCard({
  actions,
  agent,
  defaultModel,
  managedAgentRuntime,
  isBestie,
  getAvailability,
  persona,
  restartingAgentPubkey,
  startingAgentPubkey,
  startingPersonaIds,
  onOpenAgentProfile,
  onOpenPersonaProfile,
  onRestartAgent,
  onStartAgent,
  onStartPersona,
}: {
  actions?: (
    effectiveAvatarUrl: string | null,
    isEffectiveAvatarLoading: boolean,
  ) => React.ReactNode;
  agent: ManagedAgent | undefined;
  defaultModel: string;
  managedAgentRuntime?: ManagedAgentRuntimeStatus;
  isBestie: boolean;
  getAvailability: AgentAvailabilityReader;
  persona: AgentPersona;
  restartingAgentPubkey: string | null;
  startingAgentPubkey: string | null;
  startingPersonaIds: ReadonlySet<string>;
  onOpenAgentProfile: (
    pubkey: string,
    options?: ProfilePanelOpenOptions,
  ) => void;
  onOpenPersonaProfile: (persona: AgentPersona) => void;
  onRestartAgent: (pubkey: string) => void;
  onStartAgent: (pubkey: string) => void;
  onStartPersona: (persona: AgentPersona) => void;
}) {
  const availability = getAvailability(agent?.pubkey);
  const title = persona.displayName;
  const modelLabel = useAgentCardModelLabel({
    agent,
    personaModel: persona.model,
    defaultModel,
  });
  const subtitle = effectiveAgentDescription(persona) ?? modelLabel;
  const isActive = agent ? isManagedAgentActive(agent) : false;
  const profileQuery = useUserProfileQuery(agent?.pubkey);
  const avatarUrl = agent
    ? resolveAgentCardAvatarUrl(profileQuery.data?.avatarUrl, persona.avatarUrl)
    : persona.avatarUrl;
  const friendlyError = agent
    ? friendlyAgentLastError(agent.lastError, agent.lastErrorCode)?.copy
    : null;
  const showSleepingBadge = shouldShowManagedAgentSleepingBadge(
    managedAgentRuntime,
    isActive,
  );
  const noChannels = deriveNoChannelMembershipBadge(
    useNoChannelMembership(agent?.pubkey),
    agent?.status ?? "stopped",
  );
  const hasStatusBadge =
    noChannels || showSleepingBadge || Boolean(agent?.personaOrphaned);
  const defaultRuntimeId = resolveAgentDefaultRuntimeId({
    agentRuntime: agent?.runtime,
    personaRuntime: persona.runtime,
  });

  return (
    <AgentIdentityCard
      actions={actions?.(
        avatarUrl,
        isAgentCardAvatarLoading(Boolean(agent), profileQuery.isPending),
      )}
      ariaLabel={`${title} agent profile`}
      avatar={
        agent ? (
          <AgentRuntimeAvatarControl
            activeTestId={`agent-runtime-active-${agent.pubkey}`}
            avatarUrl={avatarUrl}
            errorLabel={friendlyError}
            errorTestId={`agent-runtime-error-${agent.pubkey}`}
            isActive={isActive}
            availability={availability}
            isRestarting={restartingAgentPubkey === agent.pubkey}
            isStarting={startingAgentPubkey === agent.pubkey}
            label={title}
            requiresRestart={agent.needsRestart}
            runtimeId={defaultRuntimeId}
            startTestId={`agent-runtime-start-${agent.pubkey}`}
            onOpenError={() => {
              onOpenAgentProfile(agent.pubkey, { tab: "runtime" });
            }}
            onStart={() =>
              agent.needsRestart
                ? onRestartAgent(agent.pubkey)
                : onStartAgent(agent.pubkey)
            }
          />
        ) : (
          <AgentRuntimeAvatarControl
            activeTestId={`persona-runtime-active-${persona.id}`}
            avatarUrl={avatarUrl}
            isActive={false}
            isStarting={startingPersonaIds.has(persona.id)}
            label={title}
            runtimeId={defaultRuntimeId}
            startTestId={`persona-runtime-start-${persona.id}`}
            onStart={() => onStartPersona(persona)}
          />
        )
      }
      avatarUrl={avatarUrl}
      dataTestId={`persona-agent-row-${persona.id}`}
      footerAccessory={
        agent ? (
          <ProtectedBestieCardBadge agent={agent} isBestie={isBestie} />
        ) : null
      }
      label={title}
      subtitle={subtitle}
      onClick={() => onOpenPersonaProfile(persona)}
      statusBadge={
        hasStatusBadge ? (
          <>
            {noChannels ? <AgentChannelMembershipBadge /> : null}
            {showSleepingBadge ? (
              <AgentSleepingStatusBadge
                isActive={isActive}
                runtime={managedAgentRuntime}
                testId={
                  agent ? `agent-runtime-sleeping-${agent.pubkey}` : undefined
                }
              />
            ) : null}
            {agent?.personaOrphaned ? (
              <Badge className="gap-1" variant="warning">
                <AlertTriangle className="h-3 w-3" />
                Configuration missing
              </Badge>
            ) : null}
          </>
        ) : null
      }
    />
  );
}

function StandaloneAgentCard({
  agent,
  isBestie,
  defaultModel,
  managedAgentRuntime,
  getAvailability,
  restartingAgentPubkey,
  startingAgentPubkey,
  onOpenAgentProfile,
  onRestartAgent,
  onStartAgent,
}: {
  agent: ManagedAgent;
  isBestie: boolean;
  defaultModel: string;
  managedAgentRuntime?: ManagedAgentRuntimeStatus;
  getAvailability: AgentAvailabilityReader;
  restartingAgentPubkey: string | null;
  startingAgentPubkey: string | null;
  onOpenAgentProfile: (
    pubkey: string,
    options?: ProfilePanelOpenOptions,
  ) => void;
  onRestartAgent: (pubkey: string) => void;
  onStartAgent: (pubkey: string) => void;
}) {
  const availability = getAvailability(agent.pubkey);
  const title = agent.name;
  const modelLabel = useAgentCardModelLabel({
    agent,
    personaModel: null,
    defaultModel,
  });
  const profileQuery = useUserProfileQuery(agent.pubkey);
  const friendlyError = friendlyAgentLastError(
    agent.lastError,
    agent.lastErrorCode,
  )?.copy;
  const isActive = isManagedAgentActive(agent);
  const opensRuntimeTab = Boolean(friendlyError && !isActive);
  const showSleepingBadge = shouldShowManagedAgentSleepingBadge(
    managedAgentRuntime,
    isActive,
  );
  const noChannels = deriveNoChannelMembershipBadge(
    useNoChannelMembership(agent.pubkey),
    agent.status,
  );
  const hasStatusBadge =
    noChannels ||
    Boolean(agent.profileReadiness) ||
    showSleepingBadge ||
    agent.personaOrphaned;

  return (
    <AgentIdentityCard
      ariaLabel={`${title} agent profile`}
      avatar={
        <AgentRuntimeAvatarControl
          activeTestId={`agent-runtime-active-${agent.pubkey}`}
          avatarUrl={profileQuery.data?.avatarUrl}
          errorLabel={friendlyError}
          errorTestId={`agent-runtime-error-${agent.pubkey}`}
          isActive={isActive}
          availability={availability}
          isRestarting={restartingAgentPubkey === agent.pubkey}
          isStarting={startingAgentPubkey === agent.pubkey}
          label={title}
          requiresRestart={agent.needsRestart}
          runtimeId={resolveAgentDefaultRuntimeId({
            agentRuntime: agent.runtime,
          })}
          startTestId={`agent-runtime-start-${agent.pubkey}`}
          onOpenError={() => {
            onOpenAgentProfile(agent.pubkey, { tab: "runtime" });
          }}
          onStart={() =>
            agent.needsRestart
              ? onRestartAgent(agent.pubkey)
              : onStartAgent(agent.pubkey)
          }
        />
      }
      avatarUrl={profileQuery.data?.avatarUrl}
      dataTestId={`managed-agent-${agent.pubkey}`}
      footerAccessory={
        <ProtectedBestieCardBadge agent={agent} isBestie={isBestie} />
      }
      label={title}
      subtitle={modelLabel}
      onClick={() => {
        onOpenAgentProfile(
          agent.pubkey,
          opensRuntimeTab ? { tab: "runtime" } : undefined,
        );
      }}
      statusBadge={
        hasStatusBadge ? (
          <>
            {noChannels ? <AgentChannelMembershipBadge /> : null}
            {agent.profileReadiness ? (
              <HermesProfileReadinessIndicator
                readiness={agent.profileReadiness}
              />
            ) : null}
            {showSleepingBadge ? (
              <AgentSleepingStatusBadge
                isActive={isActive}
                runtime={managedAgentRuntime}
                testId={`agent-runtime-sleeping-${agent.pubkey}`}
              />
            ) : null}
            {agent.personaOrphaned ? (
              <Badge className="gap-1" variant="warning">
                <AlertTriangle className="h-3 w-3" />
                Configuration missing
              </Badge>
            ) : null}
          </>
        ) : null
      }
    />
  );
}

function AgentSleepingStatusBadge({
  isActive,
  runtime,
  testId,
}: {
  isActive: boolean;
  runtime?: ManagedAgentRuntimeStatus;
  testId?: string;
}) {
  if (!shouldShowManagedAgentSleepingBadge(runtime, isActive)) return null;

  return (
    <Badge
      className="gap-1 normal-case tracking-normal bg-slate-500/10 text-slate-600 dark:bg-slate-400/10 dark:text-slate-300"
      data-testid={testId}
      variant="secondary"
    >
      <Moon className="h-3 w-3" />
      {MANAGED_AGENT_SLEEPING_BADGE_LABEL}
    </Badge>
  );
}

function LoadingSkeleton() {
  return (
    <div className={IDENTITY_CARD_GRID_CLASS}>
      <IdentityCardSkeleton
        footerSubtitleWidthClass="w-14"
        footerTitleWidthClass="w-24"
      />
      <IdentityCardSkeleton
        footerSubtitleWidthClass="w-20"
        footerTitleWidthClass="w-32"
      />
      <IdentityCardSkeleton
        footerSubtitleWidthClass="w-16"
        footerTitleWidthClass="w-28"
      />
    </div>
  );
}

function CollapsibleAgentGroup({
  groupKey,
  label,
  agents,
  bestiePubkey,
  collapsed,
  defaultModel,
  getAvailability,
  restartingAgentPubkey,
  runtimeByPubkey,
  startingAgentPubkey,
  onToggle,
  onOpenAgentProfile,
  onRestartAgent,
  onStartAgent,
}: {
  groupKey: string;
  label: string;
  agents: ManagedAgent[];
  bestiePubkey: string | null;
  collapsed: ReadonlySet<string>;
  defaultModel: string;
  getAvailability: AgentAvailabilityReader;
  restartingAgentPubkey: string | null;
  runtimeByPubkey: ReadonlyMap<string, ManagedAgentRuntimeStatus>;
  startingAgentPubkey: string | null;
  onToggle: (key: string) => void;
  onOpenAgentProfile: (
    pubkey: string,
    options?: ProfilePanelOpenOptions,
  ) => void;
  onRestartAgent: (pubkey: string) => void;
  onStartAgent: (pubkey: string) => void;
}) {
  const isCollapsed = collapsed.has(groupKey);
  return (
    <div className={`${AGENT_CARD_COLUMN_CLASS} space-y-2`}>
      <button
        className="group flex items-center gap-2 rounded-md px-1 py-1 text-left transition-colors hover:bg-muted/50"
        onClick={() => onToggle(groupKey)}
        type="button"
      >
        {isCollapsed ? (
          <ChevronRight className="h-4 w-4 shrink-0 text-muted-foreground transition-transform group-hover:translate-x-0.5" />
        ) : (
          <ChevronDown className="h-4 w-4 shrink-0 text-muted-foreground" />
        )}
        <span className="text-sm font-medium">{label}</span>
        <span className="text-xs text-muted-foreground">({agents.length})</span>
      </button>
      {!isCollapsed ? (
        <div className={IDENTITY_CARD_GRID_CLASS}>
          {agents.map((agent) => (
            <StandaloneAgentCard
              agent={agent}
              getAvailability={getAvailability}
              defaultModel={defaultModel}
              isBestie={agent.pubkey.toLowerCase() === bestiePubkey}
              key={agent.pubkey}
              managedAgentRuntime={runtimeByPubkey.get(
                agent.pubkey.toLowerCase(),
              )}
              restartingAgentPubkey={restartingAgentPubkey}
              startingAgentPubkey={startingAgentPubkey}
              onOpenAgentProfile={onOpenAgentProfile}
              onRestartAgent={onRestartAgent}
              onStartAgent={onStartAgent}
            />
          ))}
        </div>
      ) : null}
    </div>
  );
}

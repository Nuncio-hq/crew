/**
 * Hermes profile binding control + profile-owned model informational row.
 *
 * Rendered from the agent config field model (`hermesProfile` control /
 * `ownedByProfile` omission) — never from a hardcoded runtime id.
 */
import * as React from "react";
import { useQuery, useQueryClient } from "@tanstack/react-query";

import { readHermesProfileModel } from "@/shared/api/hermesProfiles";
import type { RespondToMode } from "@/shared/api/types";
import { cn } from "@/shared/lib/cn";
import { Button } from "@/shared/ui/button";
import { useCommunities } from "@/features/communities/useCommunities";
import {
  hermesProfilesQueryKey,
  useHermesProfilesQuery,
  useManagedAgentsQuery,
} from "../hooks";
import {
  buildHermesProfileOccupancy,
  crewMayReadHermesProfileModel,
  deriveHermesProfileUsage,
  deriveProfileBoundAgentBoundary,
  ensureHermesHomeProfileOption,
  hermesProfileBindingError,
  hermesProfileOccupancyError,
  isHermesHomeProfile,
  profileBoundAccessError,
  profileBoundBackendError,
  profileOwnedModelLabel,
  shouldShowHermesProfileCreate,
  type HermesProfileOtherUse,
  type ProfileBoundAgentBoundary,
  HERMES_HOME_PROFILE_NAME,
} from "../lib/hermesProfileBinding";
import { RequiredFieldLabel } from "./agentConfigControls";
import { useAgentRunLocation } from "./AgentRunLocationContext";
import { HermesHomeProfileConfirmDialog } from "./HermesHomeProfileConfirmDialog";
import { HermesProfileCombobox } from "./HermesProfileCombobox";
import {
  HermesProfileCreateAffordance,
  isNonOwnerOnlyRespondTo,
} from "./HermesProfileCreateAffordance";
import { HermesSoulEditor } from "./HermesSoulEditor";

export function ProfileOwnedModelRow({
  profileName,
  liveModel,
  className,
  fetchLiveModel = true,
}: {
  profileName?: string | null;
  /** Live ACP session model when a clean read path exists; otherwise omit. */
  liveModel?: string | null;
  className?: string;
  /** When true, read profile config for display when `liveModel` is absent. */
  fetchLiveModel?: boolean;
}) {
  const trimmedName = profileName?.trim() || "";
  const query = useQuery({
    queryKey: ["hermes-profile-model", trimmedName],
    queryFn: () => readHermesProfileModel(trimmedName),
    enabled:
      fetchLiveModel &&
      Boolean(trimmedName) &&
      crewMayReadHermesProfileModel(trimmedName) &&
      !liveModel?.trim(),
    refetchOnMount: "always",
  });
  const resolvedModel =
    liveModel?.trim() ||
    (query.data?.status === "ok" ? (query.data.model?.trim() ?? "") : "") ||
    null;

  return (
    <div
      className={cn("space-y-1.5", className)}
      data-testid="profile-owned-model-row"
    >
      <p className="text-sm font-medium text-foreground">Model</p>
      <p className="text-sm text-muted-foreground">
        {profileOwnedModelLabel(profileName, resolvedModel)}
      </p>
    </div>
  );
}

export function ProfileBoundAgentBoundaryCard({
  boundary,
  otherUses,
  hasPresentationMismatch,
  isBound = false,
}: {
  boundary: ProfileBoundAgentBoundary;
  otherUses: readonly HermesProfileOtherUse[];
  hasPresentationMismatch: boolean;
  isBound?: boolean;
}) {
  const rows = [
    { label: "Access", value: boundary.access },
    { label: "Autonomy", value: boundary.autonomy },
    { label: "Backend", value: boundary.backend },
    { label: "Profile", value: boundary.profile || "Choose a profile" },
    {
      label: "Used in",
      value:
        boundary.usedIn.length > 0 ? boundary.usedIn.join(", ") : "Not yet",
    },
  ];
  const otherUseText = otherUses
    .map((usage) => `${usage.agentName} in ${usage.communityName}`)
    .join(", ");

  return (
    <div
      className="space-y-3 rounded-xl border border-border/70 bg-muted/20 p-3"
      data-testid="hermes-effective-boundary"
    >
      <dl className="grid grid-cols-[auto_minmax(0,1fr)] gap-x-4 gap-y-1.5 text-sm">
        {rows.map((row) => (
          <React.Fragment key={row.label}>
            <dt className="text-muted-foreground">{row.label}</dt>
            <dd className="font-medium text-foreground">{row.value}</dd>
          </React.Fragment>
        ))}
      </dl>
      <p className="text-xs text-muted-foreground">
        Crew approves ACP tool requests automatically; the Hermes profile&apos;s
        own approval policy still applies.
      </p>
      <div
        className="space-y-1 border-t border-border/60 pt-3 text-xs text-muted-foreground"
        data-testid="hermes-profile-shared-usage"
      >
        {isBound ? (
          <p>
            One managed agent uses this profile across its configured
            communities.
          </p>
        ) : (
          <p>This profile is not bound to a managed agent yet.</p>
        )}
        <p>
          When bound, memory, skills, and profile state are shared across that
          agent&apos;s configured communities.
        </p>
        {otherUses.length > 0 ? (
          <>
            <p>Also used by {otherUseText}.</p>
            {hasPresentationMismatch ? (
              <p className="text-warning">
                This profile is presented as a different agent elsewhere; shared
                state can make those identities overlap.
              </p>
            ) : null}
          </>
        ) : null}
      </div>
    </div>
  );
}

/** Shared binding state for the field UI and create/edit save gates. */
export function useHermesProfileBindingState({
  enabled,
  hermesProfile,
  editingPubkey = null,
  currentAgentName = null,
  respondTo = null,
  required = true,
}: {
  enabled: boolean;
  hermesProfile: string;
  editingPubkey?: string | null;
  currentAgentName?: string | null;
  respondTo?: RespondToMode | null;
  required?: boolean;
}) {
  const profilesQuery = useHermesProfilesQuery({ enabled });
  const agentsQuery = useManagedAgentsQuery({ enabled });
  const { activeCommunity, communities } = useCommunities();
  const runLocation = useAgentRunLocation();
  const queryClient = useQueryClient();

  const profiles = React.useMemo(
    () => ensureHermesHomeProfileOption(profilesQuery.data ?? []),
    [profilesQuery.data],
  );

  const occupancy = React.useMemo(
    () =>
      buildHermesProfileOccupancy({
        profiles,
        agents: agentsQuery.data ?? [],
        editingPubkey,
      }),
    [profiles, agentsQuery.data, editingPubkey],
  );
  const usage = React.useMemo(
    () =>
      deriveHermesProfileUsage({
        profile: hermesProfile,
        currentAgentName,
        currentRelayUrl: activeCommunity?.relayUrl ?? "",
        editingPubkey,
        communities,
        agents: agentsQuery.data ?? [],
      }),
    [
      activeCommunity?.relayUrl,
      agentsQuery.data,
      communities,
      currentAgentName,
      editingPubkey,
      hermesProfile,
    ],
  );

  const formatError = enabled
    ? hermesProfileBindingError(hermesProfile, required)
    : null;
  const occupancyError = enabled
    ? hermesProfileOccupancyError(hermesProfile, occupancy)
    : null;
  const profileError = formatError ?? occupancyError;
  const trustedBoundaryError = enabled
    ? (profileBoundAccessError(true, respondTo) ??
      profileBoundBackendError(true, runLocation, editingPubkey !== null))
    : null;

  const invalidateProfiles = React.useCallback(() => {
    void queryClient.invalidateQueries({ queryKey: hermesProfilesQueryKey });
  }, [queryClient]);

  return {
    profiles,
    occupancy,
    usage,
    blockingError: profileError ?? trustedBoundaryError,
    profileError,
    trustedBoundaryError,
    listLoading: profilesQuery.isLoading,
    listFailed: profilesQuery.isError,
    invalidateProfiles,
  };
}

export function HermesProfileField({
  value,
  onChange,
  disabled,
  required = true,
  id = "hermes-profile",
  showValidation = true,
  enableCreateInPlace = true,
  respondTo,
  editingPubkey = null,
  currentAgentName = null,
}: {
  value: string;
  onChange: (next: string) => void;
  disabled?: boolean;
  required?: boolean;
  id?: string;
  /** When false, skip inline error (e.g. while typing before blur). */
  showValidation?: boolean;
  /** Phase 03: explicit create button (never silent on save). */
  enableCreateInPlace?: boolean;
  /** When set and not owner-only, show credential-fallback warning. */
  respondTo?: RespondToMode | null;
  /** When editing, occupancy treats this pubkey as "self". */
  editingPubkey?: string | null;
  /** Visible agent identity used only to warn about cross-community mismatch. */
  currentAgentName?: string | null;
}) {
  const {
    profiles,
    occupancy,
    usage,
    profileError,
    trustedBoundaryError,
    listLoading,
    listFailed,
    invalidateProfiles,
  } = useHermesProfileBindingState({
    enabled: true,
    hermesProfile: value,
    editingPubkey,
    currentAgentName,
    respondTo,
    required,
  });

  const boundary = deriveProfileBoundAgentBoundary({
    profileBindingOffered: true,
    profile: value,
    usedIn: usage.usedIn,
  });
  const error = showValidation ? profileError : null;
  const showPublicWarning = isNonOwnerOnlyRespondTo(respondTo);
  const showCreate =
    enableCreateInPlace && shouldShowHermesProfileCreate(value, profiles);
  const [personaStepProfile, setPersonaStepProfile] = React.useState<
    string | null
  >(null);
  const [homeConfirmOpen, setHomeConfirmOpen] = React.useState(false);

  function requestProfileChange(next: string) {
    if (isHermesHomeProfile(next)) {
      setHomeConfirmOpen(true);
      return;
    }
    onChange(next);
  }

  return (
    <div className="space-y-1.5" data-testid="hermes-profile-field">
      <RequiredFieldLabel htmlFor={id} isRequired={required}>
        Hermes profile
      </RequiredFieldLabel>
      <HermesProfileCombobox
        disabled={disabled}
        id={id}
        listFailed={listFailed}
        listLoading={listLoading}
        occupancy={occupancy}
        onChange={requestProfileChange}
        profiles={profiles}
        value={value}
      />
      <p className="text-xs text-muted-foreground">
        Pick an existing Hermes profile, the personal home profile, or type a
        new name and create it (
        <code className="font-mono text-2xs">hermes -p &lt;name&gt;</code>
        ). Binding <code className="font-mono text-2xs">default</code> requires
        confirmation — Crew will not edit that profile.
      </p>
      {boundary ? (
        <ProfileBoundAgentBoundaryCard
          boundary={boundary}
          hasPresentationMismatch={usage.hasPresentationMismatch}
          isBound={usage.isBound}
          otherUses={usage.otherUses}
        />
      ) : null}
      {showCreate ? (
        <HermesProfileCreateAffordance
          disabled={disabled}
          onCreated={(name) => {
            onChange(name);
            invalidateProfiles();
            setPersonaStepProfile(name);
          }}
          profileName={value}
          showPublicAgentWarning={showPublicWarning}
        />
      ) : showPublicWarning ? (
        <p
          className="text-xs text-muted-foreground"
          data-testid="hermes-public-agent-credential-warning"
        >
          Hermes profiles currently read the manager&apos;s pooled provider
          credentials (see docs/crew/HERMES.md).
        </p>
      ) : null}
      {personaStepProfile ? (
        <div
          className="space-y-1.5 rounded-md border border-border/60 p-3"
          data-testid="hermes-profile-persona-step"
        >
          <p className="text-sm font-medium">Set this profile&apos;s persona</p>
          <p className="text-xs text-muted-foreground">
            This optional step customizes the new profile&apos;s shared persona.
            Skip it to keep Hermes&apos; default.
          </p>
          <HermesSoulEditor profileName={personaStepProfile} />
          <Button
            onClick={() => setPersonaStepProfile(null)}
            size="sm"
            type="button"
            variant="ghost"
          >
            Skip for now
          </Button>
        </div>
      ) : null}
      {error ? (
        <p
          className="text-sm text-destructive"
          data-testid="hermes-profile-error"
        >
          {error}
        </p>
      ) : null}
      {trustedBoundaryError ? (
        <p
          className="text-sm text-destructive"
          data-testid="hermes-trusted-boundary-error"
        >
          {trustedBoundaryError}
        </p>
      ) : null}
      <HermesHomeProfileConfirmDialog
        onConfirm={() => {
          setHomeConfirmOpen(false);
          onChange(HERMES_HOME_PROFILE_NAME);
        }}
        onOpenChange={(open) => {
          setHomeConfirmOpen(open);
          if (!open && isHermesHomeProfile(value) === false) {
            return;
          }
        }}
        open={homeConfirmOpen}
      />
    </div>
  );
}

/**
 * Hermes profile binding control + profile-owned model informational row.
 *
 * Rendered from the agent config field model (`hermesProfile` control /
 * `ownedByProfile` omission) — never from a hardcoded runtime id.
 */
import * as React from "react";
import { useQueryClient } from "@tanstack/react-query";

import type { RespondToMode } from "@/shared/api/types";
import { cn } from "@/shared/lib/cn";
import { useCommunities } from "@/features/communities/useCommunities";
import {
  hermesProfilesQueryKey,
  useHermesProfilesQuery,
  useManagedAgentsQuery,
} from "../hooks";
import {
  buildHermesProfileOccupancy,
  deriveHermesProfileUsage,
  deriveProfileBoundAgentBoundary,
  hermesProfileBindingError,
  hermesProfileOccupancyError,
  normalizeHermesProfileList,
  profileBoundAccessError,
  profileBoundBackendError,
  profileOwnedModelLabel,
  shouldShowHermesProfileCreate,
  type HermesProfileOtherUse,
  type ProfileBoundAgentBoundary,
} from "../lib/hermesProfileBinding";
import { RequiredFieldLabel } from "./agentConfigControls";
import { useAgentRunLocation } from "./AgentRunLocationContext";
import { HermesProfileCombobox } from "./HermesProfileCombobox";
import {
  HermesProfileCreateAffordance,
  isNonOwnerOnlyRespondTo,
} from "./HermesProfileCreateAffordance";

export function ProfileOwnedModelRow({
  profileName,
  liveModel,
  className,
}: {
  profileName?: string | null;
  /** Live ACP session model when a clean read path exists; otherwise omit. */
  liveModel?: string | null;
  className?: string;
}) {
  return (
    <div
      className={cn("space-y-1.5", className)}
      data-testid="profile-owned-model-row"
    >
      <p className="text-sm font-medium text-foreground">Model</p>
      <p className="text-sm text-muted-foreground">
        {profileOwnedModelLabel(profileName, liveModel)}
      </p>
    </div>
  );
}

export function ProfileBoundAgentBoundaryCard({
  boundary,
  otherUses,
  hasPresentationMismatch,
}: {
  boundary: ProfileBoundAgentBoundary;
  otherUses: readonly HermesProfileOtherUse[];
  hasPresentationMismatch: boolean;
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
        <p>
          One managed agent uses this profile across its configured communities.
        </p>
        <p>Memory, skills, and profile state are shared.</p>
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
    () => normalizeHermesProfileList(profilesQuery.data ?? []),
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
        onChange={onChange}
        profiles={profiles}
        value={value}
      />
      <p className="text-xs text-muted-foreground">
        Pick an existing Hermes profile or type a new name and create it (
        <code className="font-mono text-2xs">hermes -p &lt;name&gt;</code>
        ). The manager&apos;s personal{" "}
        <code className="font-mono text-2xs">default</code> profile cannot be
        bound — see docs/crew/HERMES.md.
      </p>
      {boundary ? (
        <ProfileBoundAgentBoundaryCard
          boundary={boundary}
          hasPresentationMismatch={usage.hasPresentationMismatch}
          otherUses={usage.otherUses}
        />
      ) : null}
      {showCreate ? (
        <HermesProfileCreateAffordance
          disabled={disabled}
          onCreated={(name) => {
            onChange(name);
            invalidateProfiles();
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
    </div>
  );
}

/**
 * Hermes profile binding control + profile-owned model informational row.
 *
 * Rendered from the agent config field model (`hermesProfile` control /
 * `ownedByProfile` omission) — never from a hardcoded runtime id.
 */
import * as React from "react";
import { useQueryClient } from "@tanstack/react-query";

import { cn } from "@/shared/lib/cn";
import { useCommunities } from "@/features/communities/useCommunities";
import {
  hermesProfilesQueryKey,
  useHermesProfilesQuery,
  useManagedAgentsQuery,
} from "../hooks";
import {
  buildHermesProfileOccupancy,
  hermesProfileBindingError,
  hermesProfileOccupancyError,
  normalizeHermesProfileList,
  profileOwnedModelLabel,
  shouldShowHermesProfileCreate,
} from "../lib/hermesProfileBinding";
import { RequiredFieldLabel } from "./agentConfigControls";
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

/** Shared binding state for the field UI and create/edit save gates. */
export function useHermesProfileBindingState({
  enabled,
  hermesProfile,
  editingPubkey = null,
  required = true,
}: {
  enabled: boolean;
  hermesProfile: string;
  editingPubkey?: string | null;
  required?: boolean;
}) {
  const profilesQuery = useHermesProfilesQuery({ enabled });
  const agentsQuery = useManagedAgentsQuery({ enabled });
  const { activeCommunity } = useCommunities();
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
        relayUrl: activeCommunity?.relayUrl ?? "",
        editingPubkey,
      }),
    [profiles, agentsQuery.data, activeCommunity?.relayUrl, editingPubkey],
  );

  const formatError = enabled
    ? hermesProfileBindingError(hermesProfile, required)
    : null;
  const occupancyError = enabled
    ? hermesProfileOccupancyError(hermesProfile, occupancy)
    : null;
  const profileError = formatError ?? occupancyError;

  const invalidateProfiles = React.useCallback(() => {
    void queryClient.invalidateQueries({ queryKey: hermesProfilesQueryKey });
  }, [queryClient]);

  return {
    profiles,
    occupancy,
    profileError,
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
  respondTo?: string | null;
  /** When editing, occupancy treats this pubkey as "self". */
  editingPubkey?: string | null;
}) {
  const {
    profiles,
    occupancy,
    profileError,
    listLoading,
    listFailed,
    invalidateProfiles,
  } = useHermesProfileBindingState({
    enabled: true,
    hermesProfile: value,
    editingPubkey,
    required,
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
    </div>
  );
}

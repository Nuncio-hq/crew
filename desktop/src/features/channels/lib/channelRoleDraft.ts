import type { CanvasResponse } from "@/shared/api/types";
import type { CrewConfigDraft } from "@/shared/api/channelCrewConfig";
import { parsePubkeyInput } from "@/shared/lib/nostrUtils";

export type RoleDraft = {
  id: string;
  originalLabel: string | null;
  label: string;
  definition: string;
};
export type ChannelRoleDraft = {
  roles: RoleDraft[];
  assignments: Record<string, string>;
  preservedAssignments: Record<string, string>;
  /**
   * Source spellings for assignments that were resolved only for display.
   * Keeping this separate from `assignments` lets the form validate against a
   * canonical member key without rewriting an unchanged raw canvas entry.
   */
  sourceAssignments: Record<string, { raw: string; label: string }>;
  contact: string | null;
  routing: Record<string, string>;
  capabilities: Record<string, string[]>;
  removeRouting: string[];
  removeCapabilities: string[];
};

/** Temporary form state; the native parser remains the authority. */
export function roleKey(label: string): string {
  return label.trim().replace(/[A-Z]/g, (letter) => letter.toLowerCase());
}

export function createRoleDraft(
  canvas: CanvasResponse,
  members: readonly string[],
): ChannelRoleDraft {
  const roles = canvas.definitions.map((entry, index) => ({
    id: `stored-${index}`,
    originalLabel: entry.roleLabel,
    label: entry.roleLabel,
    definition: entry.definition,
  }));
  const preservedAssignments = { ...canvas.storedAssignments };
  const assignments: Record<string, string> = {};
  const sourceAssignments: ChannelRoleDraft["sourceAssignments"] = {};
  const counts = new Map<string, number>();
  for (const raw of Object.keys(preservedAssignments)) {
    const key = parsePubkeyInput(raw);
    if (key) counts.set(key, (counts.get(key) ?? 0) + 1);
  }
  const available = new Set(members);
  for (const [raw, label] of Object.entries(preservedAssignments)) {
    const key = parsePubkeyInput(raw);
    const role = roles.find((role) => roleKey(role.label) === roleKey(label));
    if (key && role && available.has(key) && counts.get(key) === 1) {
      assignments[key] = role.id;
      // Canonical lowercase hex is byte-equivalent to what the native writer
      // emits. Only retain a source mapping when normalization would change
      // the stored key (whitespace, npub, or spelling).
      if (raw !== key) sourceAssignments[key] = { raw, label };
      delete preservedAssignments[raw];
    }
  }
  return {
    roles,
    assignments,
    preservedAssignments,
    sourceAssignments,
    contact: canvas.contactPubkey,
    routing: { ...canvas.storedRouting },
    capabilities: { ...canvas.storedCapabilities },
    removeRouting: [],
    removeCapabilities: [],
  };
}

/**
 * Rehydrates the exact form submitted with a durable native operation.
 *
 * The relay canvas may still be the pre-save head when the dialog is opened
 * again. Rebuilding from that head would silently discard the submitted role
 * edits, so the journaled CrewConfigDraft owns the temporary form until the
 * operation is resolved.
 */
export function createRoleDraftFromSubmitted(
  canvas: CanvasResponse,
  members: readonly string[],
  submitted: CrewConfigDraft,
): ChannelRoleDraft {
  const current = createRoleDraft(canvas, members);
  const roles = submitted.definitions.map((definition, index) => {
    const currentRole = canvas.definitions.find(
      (entry) => roleKey(entry.roleLabel) === roleKey(definition.label),
    );
    const renamedFrom = Object.entries(submitted.renames).find(
      ([, to]) => roleKey(to) === roleKey(definition.label),
    )?.[0];
    return {
      id: `recovered-${index}`,
      originalLabel: currentRole?.roleLabel ?? renamedFrom ?? null,
      label: definition.label,
      definition: definition.definition,
    };
  });
  const assignments: Record<string, string> = {};
  const sourceAssignments = { ...current.sourceAssignments };
  for (const [agent, label] of Object.entries(submitted.assignments)) {
    const role = roles.find((entry) => roleKey(entry.label) === roleKey(label));
    if (role) {
      assignments[agent] = role.id;
      // An entry included in the submitted canonical map was explicitly
      // edited. It must not fall back to the source spelling on re-save.
      delete sourceAssignments[agent];
    }
  }
  return {
    ...current,
    roles,
    assignments,
    preservedAssignments: { ...submitted.preserved_assignments },
    sourceAssignments,
    contact: submitted.contact,
    removeRouting: [...submitted.remove_routing],
    removeCapabilities: [...submitted.remove_capabilities],
  };
}

export function roleDraftError(draft: ChannelRoleDraft): string | null {
  const labels = new Set<string>();
  for (const role of draft.roles) {
    const label = role.label.trim();
    if (
      !label ||
      /[\r\n]/.test(label) ||
      new TextEncoder().encode(label).length > 128
    )
      return "Each role needs a single-line name of at most 128 UTF-8 bytes.";
    const key = roleKey(label);
    if (labels.has(key)) return "Role names must be unique.";
    labels.add(key);
  }
  return null;
}

export function serializeRoleDraft(draft: ChannelRoleDraft): CrewConfigDraft {
  const error = roleDraftError(draft);
  if (error) throw new Error(error);
  const assignments: Record<string, string> = {};
  const preservedAssignments = { ...draft.preservedAssignments };
  for (const [agent, roleId] of Object.entries(draft.assignments)) {
    const role = draft.roles.find((role) => role.id === roleId);
    if (!role) throw new Error("Resolve assignments for the removed role.");
    const source = draft.sourceAssignments[agent];
    const sourceRole = source
      ? draft.roles.find(
          (candidate) =>
            candidate.originalLabel !== null &&
            roleKey(candidate.originalLabel) === roleKey(source.label),
        )
      : undefined;
    if (source && sourceRole?.id === role.id) {
      // The assignment still points at the same stored role. Preserve its
      // exact source key/value; role renames are carried by `renames` below.
      preservedAssignments[source.raw] = source.label;
    } else {
      // A direct reassignment is an explicit edit, so remove any matching
      // unresolved source entry before emitting the canonical assignment.
      for (const raw of Object.keys(preservedAssignments)) {
        if (parsePubkeyInput(raw) === agent) delete preservedAssignments[raw];
      }
      assignments[agent] = role.label.trim();
    }
  }
  const renames = Object.fromEntries(
    draft.roles.flatMap((role) =>
      role.originalLabel !== null && role.originalLabel !== role.label.trim()
        ? [[role.originalLabel, role.label.trim()]]
        : [],
    ),
  );
  return {
    definitions: draft.roles.map((role) => ({
      label: role.label.trim(),
      definition: role.definition,
    })),
    assignments,
    contact: draft.contact,
    renames,
    preserved_assignments: preservedAssignments,
    remove_routing: [...draft.removeRouting],
    remove_capabilities: [...draft.removeCapabilities],
  };
}

export function roleReferences(draft: ChannelRoleDraft, id: string) {
  const role = draft.roles.find((role) => role.id === id);
  const key = roleKey(role?.originalLabel ?? role?.label ?? "");
  return {
    holders: Object.entries(draft.assignments)
      .filter(([, roleId]) => roleId === id)
      .map(([key]) => key),
    unresolved: Object.entries(draft.preservedAssignments)
      .filter(([, label]) => roleKey(label) === key)
      .map(([raw]) => raw),
    routing: Object.entries(draft.routing)
      .filter(
        ([name, label]) =>
          roleKey(label) === key && !draft.removeRouting.includes(name),
      )
      .map(([name]) => name),
    capabilities: Object.keys(draft.capabilities).filter(
      (label) =>
        roleKey(label) === key && !draft.removeCapabilities.includes(label),
    ),
  };
}

export function removeRole(
  draft: ChannelRoleDraft,
  id: string,
  resolve: boolean,
): ChannelRoleDraft {
  const refs = roleReferences(draft, id);
  if (!resolve && Object.values(refs).some((list) => list.length))
    throw new Error(
      "Resolve holders and routing/capability references before removing this role.",
    );
  return {
    ...draft,
    roles: draft.roles.filter((role) => role.id !== id),
    assignments: Object.fromEntries(
      Object.entries(draft.assignments).filter(
        ([key]) => !refs.holders.includes(key),
      ),
    ),
    preservedAssignments: Object.fromEntries(
      Object.entries(draft.preservedAssignments).filter(
        ([raw]) => !refs.unresolved.includes(raw),
      ),
    ),
    removeRouting: [...new Set([...draft.removeRouting, ...refs.routing])],
    removeCapabilities: [
      ...new Set([...draft.removeCapabilities, ...refs.capabilities]),
    ],
  };
}

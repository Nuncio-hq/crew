import * as React from "react";

import {
  formatOrgError,
  validateOrgTree,
  type OrgNode,
  type OrgRoster,
} from "@/features/org/lib/orgRoster";
import { OFFICE_FIELD_CONTROL_CLASS } from "@/shared/layout/officeChrome";
import { truncatePubkey } from "@/shared/lib/pubkey";
import { Button } from "@/shared/ui/button";
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from "@/shared/ui/dialog";
import { OfficeField } from "@/shared/ui/OfficeField";
import { cn } from "@/shared/lib/cn";

type DraftNode = {
  rowKey: string;
  agentPubkey: string;
  manager: string;
  domain: string;
  duties: string;
  cadence: string;
  tokensPerDay: string;
  openWorkCap: string;
};

function nodeToDraft(node: OrgNode): DraftNode {
  return {
    rowKey: node.agentPubkey,
    agentPubkey: node.agentPubkey,
    manager: node.manager,
    domain: node.domain,
    duties: node.duties,
    cadence: node.cadence,
    tokensPerDay: String(node.budget.tokensPerDay),
    openWorkCap: String(node.budget.openWorkCap),
  };
}

function emptyDraft(founder: string): DraftNode {
  return {
    rowKey: `new-${crypto.randomUUID()}`,
    agentPubkey: "",
    manager: founder,
    domain: "",
    duties: "",
    cadence: "",
    tokensPerDay: "10000",
    openWorkCap: "1",
  };
}

const controlClass = cn(
  OFFICE_FIELD_CONTROL_CLASS,
  "h-9 w-full px-3 py-1 text-sm",
);

export function OrgRosterEditor({
  open,
  onOpenChange,
  roster,
  founderPubkey,
  memberPubkeys,
  names,
  onPublish,
  publishing,
}: {
  open: boolean;
  onOpenChange: (open: boolean) => void;
  roster: OrgRoster | null;
  founderPubkey: string;
  memberPubkeys: readonly string[];
  names: Record<string, string>;
  onPublish: (roster: OrgRoster) => Promise<void>;
  publishing: boolean;
}) {
  const [drafts, setDrafts] = React.useState<DraftNode[]>([]);
  const [error, setError] = React.useState<string | null>(null);

  React.useEffect(() => {
    if (!open) {
      return;
    }
    const existing = roster ? Object.values(roster.nodes).map(nodeToDraft) : [];
    setDrafts(existing.length > 0 ? existing : [emptyDraft(founderPubkey)]);
    setError(null);
  }, [founderPubkey, open, roster]);

  const managerOptions = React.useMemo(() => {
    const keys = new Set<string>([
      founderPubkey,
      ...drafts.map((row) => row.agentPubkey),
    ]);
    return [...keys].filter((key) => key.length === 64);
  }, [drafts, founderPubkey]);

  const patchDraft = (index: number, patch: Partial<DraftNode>) => {
    setDrafts((current) =>
      current.map((row, rowIndex) =>
        rowIndex === index ? { ...row, ...patch } : row,
      ),
    );
  };

  const buildRoster = React.useCallback((): OrgRoster | null => {
    const nodes: Record<string, OrgNode> = {};
    for (const draft of drafts) {
      const agent = draft.agentPubkey.trim().toLowerCase();
      if (!agent) {
        continue;
      }
      nodes[agent] = {
        agentPubkey: agent,
        manager: draft.manager.trim().toLowerCase(),
        domain: draft.domain.trim(),
        duties: draft.duties.trim(),
        cadence: draft.cadence.trim(),
        budget: {
          tokensPerDay: Number(draft.tokensPerDay),
          openWorkCap: Number(draft.openWorkCap),
        },
        officeChannel: roster?.nodes[agent]?.officeChannel ?? null,
      };
    }
    const next: OrgRoster = {
      nodes,
      founderPubkey: founderPubkey.toLowerCase(),
      eventId: null,
      createdAt: null,
    };
    const treeError = validateOrgTree(next);
    if (treeError) {
      setError(formatOrgError(treeError));
      return null;
    }
    setError(null);
    return next;
  }, [drafts, founderPubkey, roster]);

  return (
    <Dialog onOpenChange={onOpenChange} open={open}>
      <DialogContent
        className="max-h-[90vh] overflow-y-auto"
        data-testid="org-roster-editor"
      >
        <DialogHeader>
          <DialogTitle>Edit org roster</DialogTitle>
          <DialogDescription>
            Officers draft; you sign. Publish replaces the whole tree in one
            event.
          </DialogDescription>
        </DialogHeader>
        <div className="flex flex-col gap-4">
          {drafts.map((draft, index) => (
            <div
              className="rounded-lg border border-border bg-card p-3"
              key={draft.rowKey}
            >
              <OfficeField htmlFor={`org-agent-${draft.rowKey}`} label="Agent">
                <select
                  className={controlClass}
                  id={`org-agent-${draft.rowKey}`}
                  onChange={(event) =>
                    patchDraft(index, { agentPubkey: event.target.value })
                  }
                  value={draft.agentPubkey}
                >
                  <option value="">Select agent</option>
                  {memberPubkeys
                    .filter((pubkey) => pubkey !== founderPubkey)
                    .map((pubkey) => (
                      <option key={pubkey} value={pubkey}>
                        {names[pubkey] ?? truncatePubkey(pubkey)}
                      </option>
                    ))}
                </select>
              </OfficeField>
              <OfficeField
                className="mt-3"
                htmlFor={`org-manager-${draft.rowKey}`}
                label="Reports to"
              >
                <select
                  className={controlClass}
                  id={`org-manager-${draft.rowKey}`}
                  onChange={(event) =>
                    patchDraft(index, { manager: event.target.value })
                  }
                  value={draft.manager}
                >
                  {managerOptions.map((pubkey) => (
                    <option key={pubkey} value={pubkey}>
                      {pubkey === founderPubkey
                        ? "Founder"
                        : (names[pubkey] ?? truncatePubkey(pubkey))}
                    </option>
                  ))}
                </select>
              </OfficeField>
              <OfficeField
                className="mt-3"
                htmlFor={`org-domain-${draft.rowKey}`}
                label="Domain"
              >
                <input
                  className={controlClass}
                  id={`org-domain-${draft.rowKey}`}
                  onChange={(event) =>
                    patchDraft(index, { domain: event.target.value })
                  }
                  value={draft.domain}
                />
              </OfficeField>
              <OfficeField
                className="mt-3"
                htmlFor={`org-duties-${draft.rowKey}`}
                label="Duties"
              >
                <textarea
                  className={cn(
                    OFFICE_FIELD_CONTROL_CLASS,
                    "min-h-16 w-full px-3 py-2 text-sm",
                  )}
                  id={`org-duties-${draft.rowKey}`}
                  onChange={(event) =>
                    patchDraft(index, { duties: event.target.value })
                  }
                  rows={2}
                  value={draft.duties}
                />
              </OfficeField>
              <div className="mt-3 grid grid-cols-3 gap-2">
                <OfficeField
                  htmlFor={`org-cadence-${draft.rowKey}`}
                  label="Cadence"
                >
                  <input
                    className={controlClass}
                    id={`org-cadence-${draft.rowKey}`}
                    onChange={(event) =>
                      patchDraft(index, { cadence: event.target.value })
                    }
                    value={draft.cadence}
                  />
                </OfficeField>
                <OfficeField
                  htmlFor={`org-tokens-${draft.rowKey}`}
                  label="Tokens/day"
                >
                  <input
                    className={controlClass}
                    id={`org-tokens-${draft.rowKey}`}
                    onChange={(event) =>
                      patchDraft(index, { tokensPerDay: event.target.value })
                    }
                    value={draft.tokensPerDay}
                  />
                </OfficeField>
                <OfficeField
                  htmlFor={`org-cap-${draft.rowKey}`}
                  label="Open cap"
                >
                  <input
                    className={controlClass}
                    id={`org-cap-${draft.rowKey}`}
                    onChange={(event) =>
                      patchDraft(index, { openWorkCap: event.target.value })
                    }
                    value={draft.openWorkCap}
                  />
                </OfficeField>
              </div>
              <Button
                className="mt-2"
                onClick={() =>
                  setDrafts((current) =>
                    current.filter((_, rowIndex) => rowIndex !== index),
                  )
                }
                size="xs"
                type="button"
                variant="ghost"
              >
                Remove
              </Button>
            </div>
          ))}
          <Button
            onClick={() =>
              setDrafts((current) => [...current, emptyDraft(founderPubkey)])
            }
            size="sm"
            type="button"
            variant="outline"
          >
            Add agent
          </Button>
          {error ? (
            <p
              className="text-sm text-destructive"
              data-testid="org-roster-error"
            >
              {error}
            </p>
          ) : null}
        </div>
        <DialogFooter>
          <Button
            onClick={() => onOpenChange(false)}
            type="button"
            variant="ghost"
          >
            Cancel
          </Button>
          <Button
            data-testid="org-roster-publish"
            disabled={publishing}
            onClick={() => {
              const next = buildRoster();
              if (!next) {
                return;
              }
              void onPublish(next);
            }}
            type="button"
          >
            Publish roster
          </Button>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  );
}

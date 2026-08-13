import * as React from "react";

import {
  formatOrgError,
  validateOrgTree,
  type OrgNode,
  type OrgRoster,
} from "@/features/org/lib/orgRoster";
import { Button } from "@/shared/ui/button";
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from "@/shared/ui/dialog";
import { Input } from "@/shared/ui/input";
import { truncatePubkey } from "@/shared/lib/pubkey";

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
              className="rounded-lg border border-border/60 p-3"
              key={draft.rowKey}
            >
              <label
                className="block text-2xs uppercase text-muted-foreground"
                htmlFor={`org-agent-${draft.rowKey}`}
              >
                Agent
                <select
                  className="mt-1 w-full rounded-md border border-input/40 bg-background px-2 py-1 text-sm"
                  id={`org-agent-${draft.rowKey}`}
                  onChange={(event) => {
                    const value = event.target.value;
                    setDrafts((current) =>
                      current.map((row, rowIndex) =>
                        rowIndex === index
                          ? { ...row, agentPubkey: value }
                          : row,
                      ),
                    );
                  }}
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
              </label>
              <label
                className="mt-2 block text-2xs uppercase text-muted-foreground"
                htmlFor={`org-manager-${draft.rowKey}`}
              >
                Reports to
                <select
                  className="mt-1 w-full rounded-md border border-input/40 bg-background px-2 py-1 text-sm"
                  id={`org-manager-${draft.rowKey}`}
                  onChange={(event) => {
                    const value = event.target.value;
                    setDrafts((current) =>
                      current.map((row, rowIndex) =>
                        rowIndex === index ? { ...row, manager: value } : row,
                      ),
                    );
                  }}
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
              </label>
              <label
                className="mt-2 block text-2xs uppercase text-muted-foreground"
                htmlFor={`org-domain-${draft.rowKey}`}
              >
                Domain
                <Input
                  className="mt-1"
                  id={`org-domain-${draft.rowKey}`}
                  onChange={(event) => {
                    const value = event.target.value;
                    setDrafts((current) =>
                      current.map((row, rowIndex) =>
                        rowIndex === index ? { ...row, domain: value } : row,
                      ),
                    );
                  }}
                  value={draft.domain}
                />
              </label>
              <label
                className="mt-2 block text-2xs uppercase text-muted-foreground"
                htmlFor={`org-duties-${draft.rowKey}`}
              >
                Duties
                <textarea
                  className="mt-1 w-full rounded-lg border border-input/40 bg-background px-3 py-1 text-sm"
                  id={`org-duties-${draft.rowKey}`}
                  onChange={(event) => {
                    const value = event.target.value;
                    setDrafts((current) =>
                      current.map((row, rowIndex) =>
                        rowIndex === index ? { ...row, duties: value } : row,
                      ),
                    );
                  }}
                  rows={2}
                  value={draft.duties}
                />
              </label>
              <div className="mt-2 grid grid-cols-3 gap-2">
                <label
                  className="text-2xs uppercase text-muted-foreground"
                  htmlFor={`org-cadence-${draft.rowKey}`}
                >
                  Cadence
                  <Input
                    className="mt-1"
                    id={`org-cadence-${draft.rowKey}`}
                    onChange={(event) => {
                      const value = event.target.value;
                      setDrafts((current) =>
                        current.map((row, rowIndex) =>
                          rowIndex === index ? { ...row, cadence: value } : row,
                        ),
                      );
                    }}
                    value={draft.cadence}
                  />
                </label>
                <label
                  className="text-2xs uppercase text-muted-foreground"
                  htmlFor={`org-tokens-${draft.rowKey}`}
                >
                  Tokens/day
                  <Input
                    className="mt-1"
                    id={`org-tokens-${draft.rowKey}`}
                    onChange={(event) => {
                      const value = event.target.value;
                      setDrafts((current) =>
                        current.map((row, rowIndex) =>
                          rowIndex === index
                            ? { ...row, tokensPerDay: value }
                            : row,
                        ),
                      );
                    }}
                    value={draft.tokensPerDay}
                  />
                </label>
                <label
                  className="text-2xs uppercase text-muted-foreground"
                  htmlFor={`org-cap-${draft.rowKey}`}
                >
                  Open cap
                  <Input
                    className="mt-1"
                    id={`org-cap-${draft.rowKey}`}
                    onChange={(event) => {
                      const value = event.target.value;
                      setDrafts((current) =>
                        current.map((row, rowIndex) =>
                          rowIndex === index
                            ? { ...row, openWorkCap: value }
                            : row,
                        ),
                      );
                    }}
                    value={draft.openWorkCap}
                  />
                </label>
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

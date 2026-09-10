import * as React from "react";
import { Button } from "@/shared/ui/button";
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from "@/shared/ui/dialog";
import { Textarea } from "@/shared/ui/textarea";
import { useChannelRoleEditor } from "../hooks/useChannelRoleEditor";
import {
  removeRole,
  roleDraftError,
  roleReferences,
} from "../lib/channelRoleDraft";

const inputClass =
  "w-full rounded-md border border-input bg-background px-3 py-2 text-sm";

export function ChannelRolesDialog({
  channelId,
  onClose,
  onApplied,
}: {
  channelId: string;
  onClose: () => void;
  onApplied: (eventId: string | null) => void;
}) {
  const editor = useChannelRoleEditor(channelId, onApplied);
  const { draft, snapshot, progress } = editor;
  const [deleteId, setDeleteId] = React.useState<string | null>(null);
  const [focusId, setFocusId] = React.useState<string | null>(null);
  const frozen = editor.busy || !!editor.operation;
  const invalid = draft ? roleDraftError(draft) : null;
  const contactUnknown =
    !!draft?.contact &&
    !snapshot?.members.some((member) => member.pubkey === draft.contact);
  const reviewRequired =
    snapshot?.canvas.crewAuthority === "foreign" ||
    snapshot?.canvas.crewParseState === "invalid";
  const deleting = draft?.roles.find((role) => role.id === deleteId);

  return (
    <Dialog
      open
      onOpenChange={(open) => {
        if (!open && !editor.busy) onClose();
      }}
    >
      <DialogContent
        className="max-w-3xl overflow-y-auto"
        showCloseButton={!editor.busy}
        data-testid="channel-roles-dialog"
      >
        <DialogHeader>
          <DialogTitle>Channel roles</DialogTitle>
          <DialogDescription>
            Define responsibilities and choose the agents who hold each role.
            Each agent holds one role in this channel.
          </DialogDescription>
        </DialogHeader>
        {editor.busy && !draft ? (
          <p role="status">Loading current canvas and members…</p>
        ) : null}
        {editor.error ? (
          <p role="alert" className="text-sm text-destructive">
            {editor.error}
          </p>
        ) : null}
        {reviewRequired ? (
          <p role="alert" className="rounded-lg border p-3 text-sm">
            This canvas is malformed or belongs to another author. Close this
            dialog and review it in Edit canvas before changing roles.
          </p>
        ) : null}
        {draft && snapshot ? (
          <fieldset
            disabled={frozen || reviewRequired}
            className="min-w-0 space-y-4"
          >
            <label className="block space-y-1 text-sm font-medium">
              <span>Channel contact</span>
              <select
                className={inputClass}
                value={draft.contact ?? ""}
                onChange={(event) =>
                  editor.setDraft({
                    ...draft,
                    contact: event.target.value || null,
                  })
                }
              >
                <option value="">None</option>
                {contactUnknown ? (
                  <option value={draft.contact ?? ""}>
                    Unresolved contact — choose a current member or None
                  </option>
                ) : null}
                {snapshot.members.map((member) => (
                  <option key={member.pubkey} value={member.pubkey}>
                    {member.name}
                  </option>
                ))}
              </select>
            </label>
            <p className="text-xs text-muted-foreground">
              Contact is saved in the canvas. Automatic contact routing is not
              active yet. Existing agent sessions keep their current
              instructions until restarted.
            </p>
            {draft.roles.map((role, index) => (
              <section
                key={role.id}
                className="space-y-3 rounded-xl border border-border p-4"
                aria-label={`Role ${index + 1}`}
              >
                <div className="flex items-end gap-2">
                  <label className="min-w-0 flex-1 space-y-1 text-sm font-medium">
                    <span>Role name</span>
                    <input
                      className={inputClass}
                      value={role.label}
                      ref={(element) => {
                        if (element && focusId === role.id) {
                          element.focus();
                          setFocusId(null);
                        }
                      }}
                      onChange={(event) =>
                        editor.setDraft({
                          ...draft,
                          roles: draft.roles.map((entry) =>
                            entry.id === role.id
                              ? { ...entry, label: event.target.value }
                              : entry,
                          ),
                        })
                      }
                    />
                  </label>
                  <Button
                    type="button"
                    variant="outline"
                    onClick={() => {
                      const refs = roleReferences(draft, role.id);
                      if (Object.values(refs).some((list) => list.length))
                        setDeleteId(role.id);
                      else editor.setDraft(removeRole(draft, role.id, false));
                    }}
                  >
                    Remove role
                  </Button>
                </div>
                <label
                  htmlFor={`role-definition-${role.id}`}
                  className="block space-y-1 text-sm font-medium"
                >
                  <span>Responsibilities and boundaries</span>
                  <Textarea
                    id={`role-definition-${role.id}`}
                    value={role.definition}
                    placeholder="What this role can do, cannot do, and should redirect."
                    onChange={(event) =>
                      editor.setDraft({
                        ...draft,
                        roles: draft.roles.map((entry) =>
                          entry.id === role.id
                            ? { ...entry, definition: event.target.value }
                            : entry,
                        ),
                      })
                    }
                  />
                </label>
                <fieldset className="space-y-2">
                  <legend className="mb-2 text-sm font-medium">
                    Role holders
                  </legend>
                  {snapshot.members.length === 0 ? (
                    <p className="text-sm text-muted-foreground">
                      No agents are members of this channel.
                    </p>
                  ) : null}
                  {snapshot.members.map((member) => {
                    const held = draft.roles.find(
                      (entry) => entry.id === draft.assignments[member.pubkey],
                    );
                    return (
                      <label
                        key={member.pubkey}
                        className="flex items-center gap-2 text-sm"
                      >
                        <input
                          type="checkbox"
                          checked={held?.id === role.id}
                          onChange={(event) => {
                            const assignments = { ...draft.assignments };
                            if (event.target.checked)
                              assignments[member.pubkey] = role.id;
                            else delete assignments[member.pubkey];
                            editor.setDraft({ ...draft, assignments });
                          }}
                        />
                        <span>
                          {member.name}
                          {held && held.id !== role.id
                            ? ` — move from ${held.label || "unnamed role"}`
                            : ""}
                        </span>
                      </label>
                    );
                  })}
                </fieldset>
              </section>
            ))}
            <Button
              type="button"
              variant="outline"
              onClick={() => {
                const id = crypto.randomUUID();
                setFocusId(id);
                editor.setDraft({
                  ...draft,
                  roles: [
                    ...draft.roles,
                    { id, originalLabel: null, label: "", definition: "" },
                  ],
                });
              }}
            >
              Add role
            </Button>
            {Object.keys(draft.preservedAssignments).length ? (
              <section className="space-y-2 rounded-lg border p-3">
                <p className="text-sm font-medium">Unresolved assignments</p>
                <p className="text-xs text-muted-foreground">
                  These stored entries are preserved unless you remove them
                  explicitly.
                </p>
                {Object.entries(draft.preservedAssignments).map(
                  ([raw, label]) => (
                    <div key={raw} className="flex items-center gap-2 text-sm">
                      <span className="min-w-0 flex-1 break-all">
                        {raw} · {label}
                      </span>
                      <Button
                        type="button"
                        variant="outline"
                        size="sm"
                        onClick={() =>
                          editor.setDraft({
                            ...draft,
                            preservedAssignments: Object.fromEntries(
                              Object.entries(draft.preservedAssignments).filter(
                                ([key]) => key !== raw,
                              ),
                            ),
                          })
                        }
                      >
                        Remove assignment
                      </Button>
                    </div>
                  ),
                )}
              </section>
            ) : null}
          </fieldset>
        ) : null}
        {deleting && draft ? (
          <div
            role="alert"
            className="space-y-2 rounded-lg border border-destructive p-3 text-sm"
          >
            <p>
              Remove “{deleting.label}” and clear its holders, unresolved
              assignments, routing presets, and capability references from this
              draft?
            </p>
            <div className="flex gap-2">
              <Button
                type="button"
                variant="destructive"
                disabled={frozen}
                onClick={() => {
                  editor.setDraft(removeRole(draft, deleting.id, true));
                  setDeleteId(null);
                }}
              >
                Remove role and references
              </Button>
              <Button
                type="button"
                variant="outline"
                onClick={() => setDeleteId(null)}
              >
                Keep role
              </Button>
            </div>
          </div>
        ) : null}
        {invalid || contactUnknown ? (
          <p role="alert" className="text-sm text-destructive">
            {invalid ??
              "Choose a current channel member or None for the contact."}
          </p>
        ) : null}
        {editor.operation ? (
          <div
            role="status"
            className="space-y-2 rounded-lg border p-3 text-sm"
          >
            <p>
              {progress?.outcome === "canvas_committed_announcement_pending"
                ? "Canvas saved; the working-agreement announcement is still pending."
                : progress?.outcome === "superseded"
                  ? "A newer canvas replaced this operation. Review the latest canvas before starting another save."
                  : "Save recovery is pending. Your draft is retained; you can close and reopen this dialog to check recovery."}
            </p>
            {progress?.manual_retry_required ? (
              <p>
                Automatic retries have stopped. Retry when the connection is
                available.
              </p>
            ) : null}
            <div className="flex gap-2">
              <Button
                type="button"
                size="sm"
                variant="outline"
                onClick={() => {
                  void editor.refresh();
                }}
              >
                Check status
              </Button>
              <Button
                type="button"
                size="sm"
                variant="outline"
                onClick={() => {
                  void editor.refresh(true);
                }}
              >
                Retry saved operation
              </Button>
            </div>
          </div>
        ) : null}
        {editor.conflict ? (
          <Button
            type="button"
            variant="outline"
            disabled={editor.busy}
            onClick={() => {
              void editor.loadLatest();
            }}
          >
            Load latest for review
          </Button>
        ) : null}
        {editor.review ? (
          <section className="space-y-2 rounded-lg border p-3 text-sm">
            <p className="font-medium">
              Latest canvas — your draft has not changed
            </p>
            <pre className="max-h-48 overflow-auto whitespace-pre-wrap">
              {editor.review.canvas.content ?? "No canvas"}
            </pre>
            <Button
              type="button"
              variant="outline"
              disabled={
                !!editor.operation && progress?.outcome !== "superseded"
              }
              onClick={() => {
                void editor.replaceDraft();
              }}
            >
              Discard my draft and use this version
            </Button>
          </section>
        ) : null}
        <DialogFooter>
          <Button
            type="button"
            variant="outline"
            disabled={editor.busy}
            onClick={onClose}
          >
            {editor.operation ? "Close" : "Cancel"}
          </Button>
          <Button
            type="button"
            disabled={
              !draft ||
              frozen ||
              !!invalid ||
              contactUnknown ||
              reviewRequired ||
              editor.conflict ||
              !!deleteId
            }
            onClick={() => {
              void editor.save();
            }}
          >
            {editor.busy ? "Saving…" : "Save roles"}
          </Button>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  );
}

import type { EditorView } from "@tiptap/pm/view";
import { TextSelection } from "@tiptap/pm/state";

import { parseSnapshotClipboardHtml } from "./agentSnapshotClipboard";

function unwrapExactHttpLink(text: string): string | null {
  const match = /^(?:<(https?:\/\/[^\s<>]+)>|(https?:\/\/\S+))$/i.exec(text);
  return match?.[1] ?? match?.[2] ?? null;
}

/**
 * Handle paste in the composer. If the clipboard is a single plain-text URL,
 * replace the current selection with a linked URL followed by a space.
 * Snapshot-clipboard HTML is left for `parseSnapshotClipboardHtml` to handle
 * elsewhere; returning `false` lets ProseMirror/Tiptap run its default logic.
 */
export function handleComposerPaste(view: EditorView, event: Event): boolean {
  const clipboard = (event as ClipboardEvent).clipboardData;
  if (parseSnapshotClipboardHtml(clipboard?.getData("text/html") ?? ""))
    return false;

  const url = unwrapExactHttpLink(clipboard?.getData("text/plain") ?? "");
  if (!url) return false;

  const link = view.state.schema.marks.link;
  if (!link) return false;

  const { from, to } = view.state.selection;
  let transaction = view.state.tr.replaceRangeWith(
    from,
    to,
    view.state.schema.text(url, [link.create({ href: url })]),
  );
  const end = transaction.mapping.map(to);
  transaction = transaction.insertText(" ", end);
  transaction = transaction.removeMark(end, end + 1, link);
  transaction = transaction.setSelection(
    TextSelection.create(transaction.doc, end + 1),
  );
  view.dispatch(transaction.setStoredMarks([]).scrollIntoView());
  event.preventDefault();
  return true;
}

import { toast } from "sonner";

import { relayClient } from "@/shared/api/relayClient";
import { deleteMessage, uploadMediaBytes } from "@/shared/api/tauri";

import { invokeGovernor } from "./governorStore";

function imetaTag(url: string, mime: string): string[] {
  return ["imeta", `url ${url}`, `m ${mime}`];
}

export async function postCaptureEvidence(input: {
  channelId: string;
  threadRootId?: string | null;
  kind: "shot" | "clip";
  png: number[];
  filename: string;
}): Promise<{ eventId: string } | null> {
  const descriptor = await uploadMediaBytes(input.png, input.filename);
  const content =
    input.kind === "shot"
      ? `Simulator screenshot\n\n![](${descriptor.url})`
      : `Simulator clip\n\n![](${descriptor.url})`;
  const extraTags = [
    ["crew-evidence", "before-after-visual"],
    imetaTag(descriptor.url, descriptor.type || "image/png"),
  ];
  if (input.threadRootId) {
    extraTags.push(["e", input.threadRootId, "", "root"]);
  }
  const event = await relayClient.sendMessage(
    input.channelId,
    content,
    [],
    extraTags,
  );
  const eventId = (event as { id?: string } | null)?.id ?? "";
  toast("Posted as evidence", {
    duration: 5000,
    action: {
      label: "Undo",
      onClick: () => {
        if (eventId) {
          void deleteMessage(input.channelId, eventId).catch(() => undefined);
        }
      },
    },
  });
  return eventId ? { eventId } : null;
}

export async function captureSimPng(udid: string): Promise<number[]> {
  const bytes = await invokeGovernor<number[]>("sim_screenshot_png", { udid });
  return bytes;
}

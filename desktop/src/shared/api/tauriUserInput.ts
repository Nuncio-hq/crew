import { invokeTauri } from "@/shared/api/tauri";

export async function sendChannelUserInputAnswer(
  channelId: string,
  requestEventId: string,
  answers: string,
): Promise<{ eventId: string }> {
  const response = await invokeTauri<{ event_id: string }>(
    "send_channel_user_input_answer",
    { channelId, requestEventId, answers: JSON.parse(answers) },
  );
  return { eventId: response.event_id };
}

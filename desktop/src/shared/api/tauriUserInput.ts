import { invokeTauri } from "@/shared/api/tauri";
import type { UserInputAnswers } from "@/features/channels/lib/userInput";

export async function sendChannelUserInputAnswer(
  channelId: string,
  requestEventId: string,
  answers: UserInputAnswers,
): Promise<{ eventId: string }> {
  const response = await invokeTauri<{ event_id: string }>(
    "send_channel_user_input_answer",
    { channelId, requestEventId, answers },
  );
  return { eventId: response.event_id };
}

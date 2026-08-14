import * as React from "react";
import { Button } from "@/shared/ui/button";
import {
  Card,
  CardContent,
  CardDescription,
  CardFooter,
  CardHeader,
  CardTitle,
} from "@/shared/ui/card";
import { Textarea } from "@/shared/ui/textarea";
import { cn } from "@/shared/lib/cn";
import {
  buildQuestionAnswer,
  canSubmitUserInput,
  emptyUserInputDraft,
  selectUserInputOption,
  setUserInputCustom,
  type UserInputAnswerValue,
  type UserInputDraft,
  type UserInputEvent,
} from "@/features/channels/lib/userInput";

type Props = {
  item: UserInputEvent;
  currentPubkey: string;
  profiles?: Record<string, { ownerPubkey: string | null }>;
  sent?: boolean;
  resolution?: "answered" | "declined" | "cancelled";
  error?: string;
  sending?: boolean;
  onSubmit: (
    item: UserInputEvent,
    answers: Record<string, UserInputAnswerValue>,
  ) => Promise<void>;
  onSkip: (item: UserInputEvent) => Promise<void>;
  onDismiss?: (requestEventId: string) => void;
};

export function ChannelUserInputCard({
  item,
  currentPubkey,
  profiles,
  sent = false,
  resolution,
  error,
  sending = false,
  onSubmit,
  onSkip,
  onDismiss,
}: Props) {
  const [state, setState] = React.useState<Record<string, UserInputDraft>>({});
  const ownerPubkey = profiles?.[item.event.pubkey]?.ownerPubkey ?? null;
  const readOnly = ownerPubkey !== null && ownerPubkey !== currentPubkey;
  const terminal = resolution !== undefined;
  const hasAnswer = canSubmitUserInput(item.request.questions, state);

  const update = (id: string, patch: Partial<UserInputDraft>) => {
    setState((current) => ({
      ...current,
      [id]: {
        ...(current[id] ?? { selected: [], custom: "", notes: {} }),
        ...patch,
      },
    }));
  };

  const submit = async () => {
    const answers: Record<string, UserInputAnswerValue> = {};
    for (const question of item.request.questions) {
      const value = state[question.id] ?? emptyUserInputDraft();
      if (value.custom.trim() || value.selected.length > 0) {
        answers[question.id] = buildQuestionAnswer(question, value);
      }
    }
    await onSubmit(item, answers);
  };

  return (
    <Card
      className="@container min-w-0 max-h-[min(42vh,28rem)] overflow-y-auto border-primary/30 shadow-lg"
      data-testid={`channel-user-input-card-${item.event.id}`}
    >
      <CardHeader className="pb-3">
        <CardTitle className="min-w-0 truncate text-base">
          {terminal
            ? resolution === "cancelled"
              ? "Question cancelled"
              : resolution === "declined"
                ? "Question declined"
                : "Question answered"
            : sent
              ? "Answer sent"
              : "Agent question"}
        </CardTitle>
        {item.request.message ? (
          <CardDescription className="whitespace-pre-wrap text-sm text-foreground/80">
            {item.request.message}
          </CardDescription>
        ) : null}
        {readOnly ? (
          <CardDescription data-testid="channel-user-input-owner-note">
            Only the agent&apos;s owner can answer this question.
          </CardDescription>
        ) : sent ? (
          <CardDescription>Sent, waiting for the agent.</CardDescription>
        ) : terminal ? (
          <CardDescription>
            This question is no longer waiting for an answer.
          </CardDescription>
        ) : null}
      </CardHeader>
      {!sent && !terminal ? (
        <CardContent className="space-y-5">
          {item.request.questions.map((question) => {
            const value = state[question.id] ?? emptyUserInputDraft();
            return (
              <fieldset key={question.id} className="space-y-2">
                <legend className="text-sm font-medium">
                  <span className="text-muted-foreground">
                    {question.header}
                  </span>
                  <span className="mt-1 block">{question.question}</span>
                </legend>
                <div className="space-y-1.5">
                  {question.options.map((option) => {
                    const checked = value.selected.includes(option.value);
                    return (
                      <label
                        className={cn(
                          "flex items-start gap-2 rounded-md border border-border/60 p-2 text-sm",
                          checked && "border-primary/50 bg-primary/5",
                        )}
                        key={option.value}
                      >
                        <input
                          aria-label={option.label}
                          checked={checked}
                          disabled={readOnly}
                          name={question.id}
                          type={question.multi_select ? "checkbox" : "radio"}
                          onChange={() => {
                            update(
                              question.id,
                              selectUserInputOption(
                                question,
                                value,
                                option.value,
                              ),
                            );
                          }}
                        />
                        <span>
                          <span className="block">{option.label}</span>
                          {option.description ? (
                            <span className="block text-xs text-muted-foreground">
                              {option.description}
                            </span>
                          ) : null}
                        </span>
                      </label>
                    );
                  })}
                </div>
                {question.allow_custom_answer ? (
                  <Textarea
                    aria-label={`${question.header} custom answer`}
                    disabled={readOnly}
                    placeholder="Your answer"
                    value={value.custom}
                    onChange={(event) =>
                      setState((current) => ({
                        ...current,
                        [question.id]: setUserInputCustom(
                          value,
                          event.target.value,
                        ),
                      }))
                    }
                  />
                ) : null}
                {question.allow_notes && value.selected.length > 0 ? (
                  <div className="space-y-2">
                    {value.selected.map((selectedValue) => {
                      const option = question.options.find(
                        ({ value: optionValue }) =>
                          optionValue === selectedValue,
                      );
                      return (
                        <Textarea
                          aria-label={`${question.header} notes for ${option?.label ?? selectedValue}`}
                          disabled={readOnly}
                          key={selectedValue}
                          placeholder={`Notes for ${option?.label ?? selectedValue} (optional)`}
                          value={value.notes[selectedValue] ?? ""}
                          onChange={(event) =>
                            update(question.id, {
                              notes: {
                                ...value.notes,
                                [selectedValue]: event.target.value,
                              },
                            })
                          }
                        />
                      );
                    })}
                  </div>
                ) : null}
              </fieldset>
            );
          })}
        </CardContent>
      ) : null}
      {terminal && onDismiss ? (
        <CardFooter>
          <Button
            data-testid="channel-user-input-dismiss"
            type="button"
            variant="ghost"
            onClick={() => onDismiss(item.event.id)}
          >
            Dismiss
          </Button>
        </CardFooter>
      ) : null}
      {!sent && !terminal ? (
        <CardFooter className="flex-col items-stretch gap-2 [@container(min-width:21.25rem)]:flex-row [@container(min-width:21.25rem)]:items-center">
          <Button
            data-testid="channel-user-input-skip"
            disabled={readOnly || sending}
            type="button"
            variant="ghost"
            onClick={() => void onSkip(item)}
          >
            {sending ? "Sending..." : "Answer nothing"}
          </Button>
          <Button
            data-testid="channel-user-input-submit"
            disabled={readOnly || !hasAnswer || sending}
            type="button"
            onClick={() => void submit()}
          >
            {sending ? "Sending..." : "Submit"}
          </Button>
        </CardFooter>
      ) : null}
      {error ? (
        <CardDescription className="px-6 pb-4 text-destructive" role="alert">
          Could not send answer: {error}
        </CardDescription>
      ) : null}
    </Card>
  );
}

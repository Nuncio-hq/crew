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
import type {
  UserInputAnswerValue,
  UserInputEvent,
} from "@/features/channels/lib/userInput";

type Props = {
  item: UserInputEvent;
  currentPubkey: string;
  profiles?: Record<string, { ownerPubkey: string | null }>;
  sent?: boolean;
  onSubmit: (
    item: UserInputEvent,
    answers: Record<string, UserInputAnswerValue>,
  ) => Promise<void>;
  onSkip: (item: UserInputEvent) => Promise<void>;
};

type QuestionState = {
  selected: string[];
  custom: string;
  notes: Record<string, string>;
};

export function ChannelUserInputCard({
  item,
  currentPubkey,
  profiles,
  sent = false,
  onSubmit,
  onSkip,
}: Props) {
  const [state, setState] = React.useState<Record<string, QuestionState>>({});
  const ownerPubkey = profiles?.[item.event.pubkey]?.ownerPubkey ?? null;
  const readOnly = ownerPubkey !== null && ownerPubkey !== currentPubkey;
  const hasAnswer = item.request.questions.every((question) => {
    const value = state[question.id];
    return Boolean(value?.custom.trim() || value?.selected.length);
  });

  const update = (id: string, patch: Partial<QuestionState>) => {
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
      const value = state[question.id] ?? {
        selected: [],
        custom: "",
        notes: {},
      };
      if (value.custom.trim()) {
        answers[question.id] = value.custom.trim();
      } else if (question.allow_notes && Object.keys(value.notes).length > 0) {
        answers[question.id] = {
          selected: question.multi_select ? value.selected : value.selected[0],
          choice_notes: value.notes,
        };
      } else {
        answers[question.id] = question.multi_select
          ? value.selected
          : value.selected[0];
      }
    }
    await onSubmit(item, answers);
  };

  return (
    <Card
      className="max-h-[min(42vh,28rem)] overflow-y-auto border-primary/30 shadow-lg"
      data-testid={`channel-user-input-card-${item.event.id}`}
    >
      <CardHeader className="pb-3">
        <CardTitle className="text-base">
          {sent ? "Answer sent" : "Agent question"}
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
        ) : null}
      </CardHeader>
      {!sent ? (
        <CardContent className="space-y-5">
          {item.request.questions.map((question) => {
            const value = state[question.id] ?? {
              selected: [],
              custom: "",
              notes: {},
            };
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
                            const selected = question.multi_select
                              ? checked
                                ? value.selected.filter(
                                    (v) => v !== option.value,
                                  )
                                : [...value.selected, option.value]
                              : [option.value];
                            update(question.id, { selected });
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
                      update(question.id, {
                        custom: event.target.value,
                        selected: [],
                      })
                    }
                  />
                ) : null}
                {question.allow_notes && value.selected.length > 0 ? (
                  <Textarea
                    aria-label={`${question.header} notes`}
                    disabled={readOnly}
                    placeholder="Notes (optional)"
                    value={Object.values(value.notes)[0] ?? ""}
                    onChange={(event) =>
                      update(question.id, {
                        notes: {
                          [value.selected[0]]: event.target.value,
                        },
                      })
                    }
                  />
                ) : null}
              </fieldset>
            );
          })}
        </CardContent>
      ) : null}
      {!sent ? (
        <CardFooter className="gap-2">
          <Button
            data-testid="channel-user-input-skip"
            disabled={readOnly}
            type="button"
            variant="ghost"
            onClick={() => void onSkip(item)}
          >
            Skip
          </Button>
          <Button
            data-testid="channel-user-input-submit"
            disabled={readOnly || !hasAnswer}
            type="button"
            onClick={() => void submit()}
          >
            Submit
          </Button>
        </CardFooter>
      ) : null}
    </Card>
  );
}

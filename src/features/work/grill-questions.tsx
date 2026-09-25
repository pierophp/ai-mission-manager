import { useState } from "react";
import { CheckIcon } from "lucide-react";
import { cn } from "cn";
import { Alert, AlertDescription, AlertTitle } from "../../components/ui/alert";
import { Button } from "../../components/ui/button";
import { Textarea } from "../../components/ui/textarea";
import type { GrillAnswer, Run } from "../../runtime/types";
import type { GrillOption, GrillQuestion } from "../../runtime/execution-types";

export function GrillQuestionFlow({
  run,
  answers,
  disabled,
  onAnswerChange,
  onSubmit,
}: {
  run: Run;
  answers: Record<number, string>;
  disabled: boolean;
  onAnswerChange: (questionNumber: number, answer: string) => void;
  onSubmit: (answers: GrillAnswer[]) => Promise<unknown>;
}) {
  const group = run.grill_question_group;
  const [isSubmitting, setIsSubmitting] = useState(false);

  if (!group || group.questions.length === 0) {
    return (
      <Alert className="border-amber-500/30 bg-amber-500/5">
        <AlertTitle>Grill is waiting for your input</AlertTitle>
        <AlertDescription>
          The question markers were incomplete or could not be parsed. The raw
          transcript is retained; continue safely through the embedded terminal.
        </AlertDescription>
      </Alert>
    );
  }
  const questionGroup = group;

  const unanswered = questionGroup.questions.some(
    (question) => !answers[question.number]?.trim(),
  );

  async function submitAnswers() {
    if (unanswered || disabled || isSubmitting) return;
    setIsSubmitting(true);
    try {
      await onSubmit(
        questionGroup.questions.map((question) => ({
          questionNumber: question.number,
          answer: answers[question.number].trim(),
        })),
      );
    } finally {
      setIsSubmitting(false);
    }
  }

  const answeredCount = questionGroup.questions.filter(
    (question) => answers[question.number]?.trim(),
  ).length;

  return (
    <section className="grid gap-4">
      <ol className="grid gap-3">
        {questionGroup.questions.map((question, index) => (
          <GrillQuestionCard
            key={`${questionGroup.round}:${question.number}:${index}`}
            question={question}
            answer={answers[question.number] ?? ""}
            disabled={disabled || isSubmitting}
            onAnswerChange={(answer) => onAnswerChange(question.number, answer)}
          />
        ))}
      </ol>
      <div className="sticky bottom-0 -mx-1 flex items-center justify-between gap-3 border-t bg-background/95 px-1 pt-3 backdrop-blur">
        <span className="text-xs text-muted-foreground tabular-nums">
          {answeredCount} of {questionGroup.questions.length} answered
        </span>
        <Button
          type="button"
          disabled={disabled || isSubmitting || unanswered}
          onClick={() => void submitAnswers()}
        >
          {isSubmitting ? "Sending grouped response…" : "Submit all answers"}
        </Button>
      </div>
    </section>
  );
}

const GRILL_RECOMMENDATION_ANSWER = "ok";

function grillOptionAnswer(option: GrillOption) {
  return `${option.key}. ${option.label}`;
}

function GrillQuestionCard({
  question,
  answer,
  disabled,
  onAnswerChange,
}: {
  question: GrillQuestion;
  answer: string;
  disabled: boolean;
  onAnswerChange: (answer: string) => void;
}) {
  const acceptedRecommendation =
    Boolean(question.recommendation) && answer === GRILL_RECOMMENDATION_ANSWER;
  const selectedOption = question.options.find(
    (option) => answer === grillOptionAnswer(option),
  );
  const customAnswer = acceptedRecommendation || selectedOption ? "" : answer;
  const isAnswered = answer.trim().length > 0;

  return (
    <li
      className={cn(
        "grid gap-3 rounded-lg border bg-background/60 p-4 transition-colors",
        isAnswered && "border-primary/30",
      )}
    >
      <div className="flex items-start gap-3">
        <span
          aria-hidden="true"
          className={cn(
            "flex size-6 shrink-0 items-center justify-center rounded-full border text-xs font-medium tabular-nums",
            isAnswered
              ? "border-primary bg-primary text-primary-foreground"
              : "text-muted-foreground",
          )}
        >
          {isAnswered ? <CheckIcon className="size-3.5" /> : question.number}
        </span>
        <div className="grid min-w-0 gap-1">
          <h3 className="text-sm font-medium leading-snug text-pretty">
            {question.title ?? question.prompt}
          </h3>
          {question.title && (
            <p className="whitespace-pre-line text-sm leading-relaxed text-pretty text-muted-foreground">
              {question.prompt}
            </p>
          )}
        </div>
      </div>

      <div className="grid gap-2 sm:ps-9">
        {question.recommendation && (
          <div
            className={cn(
              "flex items-start gap-3 rounded-md border border-dashed px-3 py-2.5 text-sm",
              acceptedRecommendation
                ? "border-primary/50 bg-primary/5"
                : "bg-muted/30",
            )}
          >
            <div className="grid min-w-0 flex-1 gap-0.5">
              <span className="text-xs font-medium uppercase tracking-wide text-muted-foreground">
                Recommendation
              </span>
              <span className="leading-snug text-pretty">
                {question.recommendation}
              </span>
            </div>
            <Button
              type="button"
              size="sm"
              variant={acceptedRecommendation ? "default" : "outline"}
              className="shrink-0"
              aria-pressed={acceptedRecommendation}
              disabled={disabled}
              onClick={() =>
                onAnswerChange(
                  acceptedRecommendation ? "" : GRILL_RECOMMENDATION_ANSWER,
                )
              }
            >
              {acceptedRecommendation && <CheckIcon />}
              {acceptedRecommendation ? "Accepted" : "Accept"}
            </Button>
          </div>
        )}

        {question.options.length > 0 && (
          <div
            role="radiogroup"
            aria-label={`Options for question ${question.number}`}
            className="grid gap-1.5"
          >
            {question.options.map((option) => {
              const checked = selectedOption?.key === option.key;
              return (
                <button
                  type="button"
                  role="radio"
                  aria-checked={checked}
                  key={option.key}
                  disabled={disabled}
                  onClick={() =>
                    onAnswerChange(checked ? "" : grillOptionAnswer(option))
                  }
                  className={cn(
                    "flex items-start gap-2.5 rounded-md border px-3 py-2 text-start text-sm transition-colors outline-none hover:bg-muted/50 focus-visible:border-ring focus-visible:ring-3 focus-visible:ring-ring/50 disabled:pointer-events-none disabled:opacity-50",
                    checked && "border-primary/50 bg-primary/5",
                  )}
                >
                  <span
                    className={cn(
                      "flex size-5 shrink-0 items-center justify-center rounded border font-mono text-[0.625rem] font-medium",
                      checked
                        ? "border-primary bg-primary text-primary-foreground"
                        : "text-muted-foreground",
                    )}
                  >
                    {option.key}
                  </span>
                  <span className="leading-snug text-pretty">{option.label}</span>
                </button>
              );
            })}
          </div>
        )}

        <Textarea
          aria-label={`Answer for question ${question.number}`}
          value={customAnswer}
          onChange={(event) => onAnswerChange(event.target.value)}
          rows={2}
          placeholder={
            question.recommendation || question.options.length > 0
              ? "Or write a different answer"
              : "Write your answer"
          }
          disabled={disabled}
        />
      </div>
    </li>
  );
}

import React, { useState, useEffect, useRef } from "react";
import { Check, CircleHelp, CornerDownLeft, Edit3 } from "lucide-react";

export interface UserQueryOption {
  label: string;
  description: string;
  preview?: string;
}

export interface UserQuestion {
  question: string;
  header: string;
  options: UserQueryOption[];
  multiSelect?: boolean;
}

export interface UserQuery {
  questions: UserQuestion[];
}

export function AskUserActions({
  query,
  appLang,
  onResolve
}: {
  query: UserQuery;
  appLang: string;
  onResolve: (answer: string) => void;
}) {
  const isZh = appLang === "zh";
  const [questionIndex, setQuestionIndex] = useState(0);
  const question = query.questions[Math.min(questionIndex, Math.max(query.questions.length - 1, 0))];

  const [selectedIndex, setSelectedIndex] = useState(0);
  const [checkedIndices, setCheckedIndices] = useState<number[]>([]);
  const [answers, setAnswers] = useState<Record<string, string | string[]>>({});
  const [manualAnswer, setManualAnswer] = useState("");
  const [isSubmitting, setIsSubmitting] = useState(false);
  const optionRefs = useRef<Array<HTMLButtonElement | null>>([]);
  const manualInputRef = useRef<HTMLTextAreaElement | null>(null);
  const hasOptions = question.options.length > 0;

  const handleToggle = (index: number) => {
    if (question.multiSelect) {
      setCheckedIndices((prev) =>
        prev.includes(index) ? prev.filter((i) => i !== index) : [...prev, index]
      );
    } else {
      setSelectedIndex(index);
    }
  };

  const resolveQuestion = (value: string | string[]) => {
    const key = question.header || question.question;
    const nextAnswers = { ...answers };
    nextAnswers[key] = value;

    if (questionIndex + 1 < query.questions.length) {
      setAnswers(nextAnswers);
      setQuestionIndex((index) => index + 1);
      setSelectedIndex(0);
      setCheckedIndices([]);
      setManualAnswer("");
    } else {
      setIsSubmitting(true);
      onResolve(JSON.stringify(nextAnswers));
    }
  };

  const submitAnswer = (idx?: number) => {
    if (!hasOptions) {
      const clean = manualAnswer.trim();
      if (clean) resolveQuestion(clean);
      return;
    }

    const targetIdx = idx !== undefined ? idx : selectedIndex;
    if (question.multiSelect) {
      const selectedLabels = checkedIndices.map((i) => question.options[i].label);
      resolveQuestion(selectedLabels);
    } else {
      const selectedOption = question.options[targetIdx];
      if (selectedOption) resolveQuestion(selectedOption.label);
    }
  };

  const submitManualAnswer = () => {
    const clean = manualAnswer.trim();
    if (clean) resolveQuestion(clean);
  };

  useEffect(() => {
    setQuestionIndex(0);
    setSelectedIndex(0);
    setCheckedIndices([]);
    setAnswers({});
    setManualAnswer("");
    setIsSubmitting(false);
  }, [query]);

  useEffect(() => {
    if (hasOptions) {
      optionRefs.current[selectedIndex]?.focus();
    } else {
      manualInputRef.current?.focus();
    }
  }, [selectedIndex, query]);

  useEffect(() => {
    const handleKeyDown = (e: KeyboardEvent) => {
      const target = e.target as HTMLElement | null;
      if (target?.closest("textarea, input, [contenteditable=true]")) return;
      if (!hasOptions) return;
      if (e.key === "ArrowUp") {
        e.preventDefault();
        setSelectedIndex((index) => (index - 1 + question.options.length) % question.options.length);
      } else if (e.key === "ArrowDown") {
        e.preventDefault();
        setSelectedIndex((index) => (index + 1) % question.options.length);
      } else if (e.key === " ") {
        if (question.multiSelect) {
          e.preventDefault();
          handleToggle(selectedIndex);
        }
      } else if (e.key === "Enter") {
        e.preventDefault();
        submitAnswer();
      }
    };

    window.addEventListener("keydown", handleKeyDown);
    return () => window.removeEventListener("keydown", handleKeyDown);
  }, [selectedIndex, checkedIndices, query, question, hasOptions]);

  return (
    <div className="ask-user-card" role="dialog" aria-modal="false" aria-labelledby="ask-user-title">
      <div className="ask-user-card-header">
        <div className="ask-user-icon">
          <CircleHelp size={17} />
        </div>
        <div>
          <div id="ask-user-title" className="ask-user-title">
            {question.header || (isZh ? "需要你的回复" : "Question")}
          </div>
          {query.questions.length > 1 && (
            <div className="ask-user-progress">
              {questionIndex + 1}/{query.questions.length}
            </div>
          )}
        </div>
      </div>
      <p className="ask-user-question">{question.question}</p>
      {hasOptions ? (
        <div className="ask-user-option-list">
          {question.options.map((option, index) => {
            const isSelected = selectedIndex === index;
            const isChecked = question.multiSelect ? checkedIndices.includes(index) : isSelected;
            return (
              <button
                className={`ask-user-option ${isChecked ? "selected" : ""}`}
                key={option.label}
                ref={(node) => {
                  optionRefs.current[index] = node;
                }}
                onClick={() => {
                  if (question.multiSelect) {
                    handleToggle(index);
                  } else {
                    setSelectedIndex(index);
                  }
                }}
                onDoubleClick={() => {
                  if (!question.multiSelect) submitAnswer(index);
                }}
                type="button"
                disabled={isSubmitting}
              >
                <span className="ask-user-option-key">
                  {question.multiSelect ? (checkedIndices.includes(index) ? <Check size={13} /> : "") : index + 1}
                </span>
                <span className="ask-user-option-main">{option.label}</span>
                {option.description ? <span className="ask-user-option-desc">{option.description}</span> : null}
              </button>
            );
          })}
        </div>
      ) : null}
      <div className="ask-user-manual">
        <div className="ask-user-manual-label">
          <Edit3 size={13} />
          <span>{isZh ? "手动输入" : "Custom answer"}</span>
        </div>
        <textarea
          ref={manualInputRef}
          value={manualAnswer}
          onChange={(event) => setManualAnswer(event.target.value)}
          disabled={isSubmitting}
          placeholder={
            hasOptions
              ? (isZh ? "也可以直接输入自定义答案..." : "Or type a custom answer...")
              : (isZh ? "输入你的回答..." : "Type your answer...")
          }
          onKeyDown={(event) => {
            if (event.key === "Enter" && (event.metaKey || event.ctrlKey)) {
              event.preventDefault();
              submitManualAnswer();
            }
          }}
        />
      </div>
      <div className="ask-user-footer">
        <button
          className="ask-user-secondary"
          onClick={submitManualAnswer}
          type="button"
          disabled={isSubmitting || !manualAnswer.trim()}
        >
          {isZh ? "提交手动回复" : "Submit custom"}
          <span>⌘↵</span>
        </button>
        <button
          className="ask-user-submit"
          onClick={() => submitAnswer()}
          type="button"
          disabled={isSubmitting || (!hasOptions && !manualAnswer.trim())}
        >
          {isSubmitting ? (isZh ? "提交中" : "Submitting") : (isZh ? "提交" : "Submit")}
          <CornerDownLeft size={14} />
        </button>
      </div>
    </div>
  );
}

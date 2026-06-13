import React, { useState, useEffect, useRef } from "react";
import { CircleDot } from "lucide-react";

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
  const optionRefs = useRef<Array<HTMLButtonElement | null>>([]);

  const handleToggle = (index: number) => {
    if (question.multiSelect) {
      setCheckedIndices((prev) =>
        prev.includes(index) ? prev.filter((i) => i !== index) : [...prev, index]
      );
    } else {
      setSelectedIndex(index);
    }
  };

  const submitAnswer = (idx?: number) => {
    const targetIdx = idx !== undefined ? idx : selectedIndex;
    const key = question.header || question.question;
    const nextAnswers = { ...answers };
    if (question.multiSelect) {
      const selectedLabels = checkedIndices.map((i) => question.options[i].label);
      nextAnswers[key] = selectedLabels;
    } else {
      const selectedOption = question.options[targetIdx];
      nextAnswers[key] = selectedOption.label;
    }

    if (questionIndex + 1 < query.questions.length) {
      setAnswers(nextAnswers);
      setQuestionIndex((index) => index + 1);
      setSelectedIndex(0);
      setCheckedIndices([]);
    } else {
      onResolve(JSON.stringify(nextAnswers));
    }
  };

  useEffect(() => {
    setQuestionIndex(0);
    setSelectedIndex(0);
    setCheckedIndices([]);
    setAnswers({});
  }, [query]);

  useEffect(() => {
    optionRefs.current[selectedIndex]?.focus();
  }, [selectedIndex, query]);

  useEffect(() => {
    const handleKeyDown = (e: KeyboardEvent) => {
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
  }, [selectedIndex, checkedIndices, query, question]);

  return (
    <div className="permission-prompt">
      <div className="permission-prompt-title">
        <CircleDot size={16} />
        <span>{question.header || (isZh ? "文件提问" : "Question")}</span>
      </div>
      {query.questions.length > 1 && (
        <div style={{ marginTop: "6px", fontSize: "12px", color: "var(--muted)" }}>
          {questionIndex + 1}/{query.questions.length}
        </div>
      )}
      <p style={{ margin: "9px 0 12px", fontSize: "13px", color: "var(--text)" }}>{question.question}</p>
      <div className="permission-option-list">
        {question.options.map((option, index) => {
          const isSelected = selectedIndex === index;
          const isChecked = question.multiSelect ? checkedIndices.includes(index) : isSelected;
          return (
            <button
              className={`permission-option ${isChecked ? "selected" : ""}`}
              key={option.label}
              ref={(node) => {
                optionRefs.current[index] = node;
              }}
              onClick={() => {
                if (question.multiSelect) {
                  handleToggle(index);
                } else {
                  submitAnswer(index);
                }
              }}
              type="button"
              style={{ outline: "none", boxShadow: "none", cursor: "pointer" }}
            >
              <kbd>{question.multiSelect ? (checkedIndices.includes(index) ? "✓" : " ") : index + 1}</kbd>
              <span>{option.label}</span>
              <em>{option.description}</em>
            </button>
          );
        })}
      </div>
      <div className="permission-prompt-footer">
        <button
          className="permission-submit"
          onClick={() => submitAnswer()}
          type="button"
          style={{ outline: "none", boxShadow: "none", cursor: "pointer" }}
        >
          {isZh ? "提交" : "Submit"}
          <span>↵</span>
        </button>
      </div>
    </div>
  );
}

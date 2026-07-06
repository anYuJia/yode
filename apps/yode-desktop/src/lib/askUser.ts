import { recordFromUnknown } from "./jsonUtils";

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

export function isUserQuery(value: unknown): value is UserQuery {
  const record = recordFromUnknown(value);
  if (!record) return false;
  return Array.isArray(record.questions) && record.questions.every(isUserQuestion);
}

export function parseUserQueryJson(raw: string): UserQuery | null {
  try {
    const parsed: unknown = JSON.parse(raw);
    return isUserQuery(parsed) ? parsed : null;
  } catch {
    return null;
  }
}

export function formatAskUserAnswerForDisplay(answer: string): string {
  try {
    const parsed: unknown = JSON.parse(answer);
    const record = recordFromUnknown(parsed);
    if (!record) return answer;
    const values = Object.values(record)
      .map(formatAnswerValue)
      .filter((value) => value.length > 0);
    return values.length > 0 ? values.join(", ") : answer;
  } catch {
    return answer;
  }
}

function isUserQuestion(value: unknown): value is UserQuestion {
  const record = recordFromUnknown(value);
  if (!record) return false;
  return (
    typeof record.question === "string" &&
    typeof record.header === "string" &&
    Array.isArray(record.options) &&
    record.options.every(isUserQueryOption) &&
    (record.multiSelect === undefined || typeof record.multiSelect === "boolean")
  );
}

function isUserQueryOption(value: unknown): value is UserQueryOption {
  const record = recordFromUnknown(value);
  if (!record) return false;
  return (
    typeof record.label === "string" &&
    typeof record.description === "string" &&
    (record.preview === undefined || typeof record.preview === "string")
  );
}

function formatAnswerValue(value: unknown): string {
  if (typeof value === "string") {
    return value;
  }
  if (Array.isArray(value)) {
    return value.filter((item): item is string => typeof item === "string").join(", ");
  }
  if (typeof value === "number" || typeof value === "boolean") {
    return String(value);
  }
  return "";
}

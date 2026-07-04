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
  if (!value || typeof value !== "object" || Array.isArray(value)) {
    return false;
  }
  const record = value as Record<string, unknown>;
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
    if (!parsed || typeof parsed !== "object" || Array.isArray(parsed)) {
      return answer;
    }
    const values = Object.values(parsed as Record<string, unknown>)
      .map(formatAnswerValue)
      .filter((value) => value.length > 0);
    return values.length > 0 ? values.join(", ") : answer;
  } catch {
    return answer;
  }
}

function isUserQuestion(value: unknown): value is UserQuestion {
  if (!value || typeof value !== "object" || Array.isArray(value)) {
    return false;
  }
  const record = value as Record<string, unknown>;
  return (
    typeof record.question === "string" &&
    typeof record.header === "string" &&
    Array.isArray(record.options) &&
    record.options.every(isUserQueryOption) &&
    (record.multiSelect === undefined || typeof record.multiSelect === "boolean")
  );
}

function isUserQueryOption(value: unknown): value is UserQueryOption {
  if (!value || typeof value !== "object" || Array.isArray(value)) {
    return false;
  }
  const record = value as Record<string, unknown>;
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

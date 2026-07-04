export function recordFromUnknown(value: unknown): Record<string, unknown> | undefined {
  return value && typeof value === "object" && !Array.isArray(value)
    ? value as Record<string, unknown>
    : undefined;
}

export function parseJsonObject(text?: string | null): Record<string, unknown> | null {
  if (!text || typeof text !== "string") return null;
  try {
    const parsed: unknown = JSON.parse(text);
    return recordFromUnknown(parsed) ?? null;
  } catch {
    return null;
  }
}

export function parseJsonArray(text?: string | null): unknown[] {
  if (!text || typeof text !== "string") return [];
  try {
    const parsed: unknown = JSON.parse(text);
    return Array.isArray(parsed) ? parsed : [];
  } catch {
    return [];
  }
}

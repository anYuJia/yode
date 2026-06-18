export function hasCodeBlockContent(text: string | null | undefined) {
  return Boolean(text && text.replace(/\u200B/g, "").trim().length > 0);
}

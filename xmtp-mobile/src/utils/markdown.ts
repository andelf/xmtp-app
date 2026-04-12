const MARKDOWN_PREVIEW_MAX_LENGTH = 80;

/**
 * Extract a short plain-text preview from raw markdown content.
 * Removes front matter, fences, and common markdown markers.
 */
export function extractMarkdownPreview(raw: string): string | undefined {
  let text = raw.trim();

  if (!text) return undefined;

  // Strip YAML front matter (--- ... ---)
  if (text.startsWith("---")) {
    const end = text.indexOf("\n---", 3);
    if (end !== -1) {
      text = text.slice(end + 4).trimStart();
    }
  }

  const parts: string[] = [];

  for (const line of text.split("\n")) {
    const trimmed = line.trim();
    if (!trimmed) continue;
    if (/^```/.test(trimmed) || /^~~~/.test(trimmed)) continue;

    const cleaned = trimmed
      .replace(/^#{1,6}\s+/, "")
      .replace(/^>\s+/, "")
      .replace(/^\d+[.)]\s+/, "")
      .replace(/^[-*+]\s+/, "")
      .replace(/^\[[ xX]\]\s+/, "")
      .replace(/!\[([^\]]*)\]\([^)]+\)/g, "$1")
      .replace(/\[([^\]]+)\]\([^)]+\)/g, "$1")
      .replace(/`([^`]+)`/g, "$1")
      .replace(/[*_~]+/g, "")
      .replace(/\s+/g, " ")
      .trim();

    if (!cleaned) continue;
    parts.push(cleaned);

    if (parts.join(" ").length >= MARKDOWN_PREVIEW_MAX_LENGTH) {
      break;
    }
  }

  if (parts.length === 0) return undefined;

  const preview = parts.join(" ").trim();
  if (preview.length <= MARKDOWN_PREVIEW_MAX_LENGTH) return preview;
  return `${preview.slice(0, MARKDOWN_PREVIEW_MAX_LENGTH - 1).trimEnd()}…`;
}

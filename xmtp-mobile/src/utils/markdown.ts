/**
 * Extract a plain-text preview line from raw markdown content.
 * Strips YAML front matter, heading markers, list bullets, and blank lines.
 * Returns the first non-empty line after stripping, or undefined.
 */
export function extractMarkdownPreview(raw: string): string | undefined {
  let text = raw;

  // Strip YAML front matter (--- ... ---)
  if (text.startsWith("---")) {
    const end = text.indexOf("---", 3);
    if (end !== -1) {
      text = text.slice(end + 3);
    }
  }

  for (const line of text.split("\n")) {
    // Strip heading markers, list bullets/dashes, and leading whitespace
    const cleaned = line
      .replace(/^#{1,6}\s+/, "")
      .replace(/^[\s]*[-*+]\s+/, "")
      .trim();
    if (cleaned) return cleaned;
  }

  return undefined;
}

import { extractMarkdownPreview } from "../markdown";

describe("extractMarkdownPreview", () => {
  it("skips fenced code block markers and keeps code content", () => {
    const raw = "```text\nhello world\n```";

    expect(extractMarkdownPreview(raw)).toBe("hello world");
  });

  it("strips markdown syntax from links and inline code", () => {
    const raw = "See [`docs`](https://example.com) for `usage` details";

    expect(extractMarkdownPreview(raw)).toBe("See docs for usage details");
  });

  it("strips heading and list markers and joins concise content", () => {
    const raw = "# Title\n\n- first item\n- second item";

    expect(extractMarkdownPreview(raw)).toBe("Title first item second item");
  });

  it("truncates long previews", () => {
    const raw = `# Heading\n\n${"a".repeat(120)}`;
    const preview = extractMarkdownPreview(raw);

    expect(preview).toBeDefined();
    expect(preview!.endsWith("…")).toBe(true);
    expect(preview!.length).toBeLessThanOrEqual(80);
  });
});

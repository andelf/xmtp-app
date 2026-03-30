# ADR-004: Use react-native-enriched-markdown for markdown message rendering

## Status
Accepted

## Context
XMTP agents send messages with content type `xmtp.org/markdown:1.0`. These arrive as base64-encoded markdown in `nativeContent.encoded`. The app needs to render formatted markdown (headings, code blocks, tables, lists, links) within message bubbles.

## Decision
Use `react-native-enriched-markdown` (EnrichedMarkdownText) with GitHub flavor for rendering. Content type is detected via `item.contentType.includes("markdown")`. Plain text messages continue using React Native `Text`.

## Alternatives considered
1. **react-native-markdown-display** — Pure JS, no native dependency. But worse performance, limited table support, no streaming animation.
2. **Custom regex-based formatting** — Only handles simple cases (bold, italic). Tables and code blocks would be extremely complex.
3. **WebView with markdown-it** — Heavy, poor integration with chat UI, breaks inverted list performance.
4. **Render as plain text** — Functional but loses all formatting intent from the sender.

## Consequences
- Adds a native dependency — requires `npx expo prebuild` / gradle rebuild (not Expo Go compatible)
- Tables rendered natively may still exceed bubble width; mitigated by compact table styling (12px font, tight padding) and wider bubble maxWidth for markdown
- ~~Horizontal ScrollView around the native component doesn't work~~ — **Resolved**: The library's internal `HorizontalScrollView` works after patching `TableContainerView.kt` to override `onInterceptTouchEvent` with `requestDisallowInterceptTouchEvent`, preventing RN's gesture system from stealing horizontal swipes. See [solution doc](../solutions/ui-bugs/markdown-table-horizontal-scroll-gesture-conflict-2026-03-29.md)
- Dark theme styles must be maintained for both own/other bubble variants
- `flavor="github"` enables tables and task lists but uses a container-based renderer (slightly different layout behavior than CommonMark single-TextView mode)

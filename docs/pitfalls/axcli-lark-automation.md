# axcli Lark Automation Pitfalls

## Background

`axcli` is a Rust CLI tool that uses macOS Accessibility API to automate Lark desktop app. Used for reading messages, navigating chats, extracting group info, and sending messages via accessibility tree inspection and interaction.

Skill reference: `~/Daily/.claude/skills/axcli-lark-skill/`

## Pitfall 1: Default snapshot depth is useless at top level

`axcli snapshot --app Lark --depth 4` from the application root shows nothing but `... N more` collapsed nodes. Lark's DOM is deeply nested — useful content is 8-12 levels deep.

**Workaround**: Always use a specific locator to narrow scope first:
```bash
# Bad — wastes a call
axcli snapshot --app Lark --depth 4

# Good — targets the actual content
axcli snapshot --app Lark '.a11y_feed_card_item >> nth=0' --depth 4
```

**Best practice**: Screenshot first to understand current state, then use targeted locators.

## Pitfall 2: `get text` mixes badge counts with element text

`get text` returns the full subtree text including badge/unread count elements. There's no way to exclude child elements.

Example: a shortcut chat with 476 unread messages returns:
```
476
Web3 Feedback Channel
```

Instead of just `Web3 Feedback Channel`.

**Impact**: Programmatic parsing requires heuristics to separate badge numbers from actual names (e.g., first line is numeric = badge count).

**Workaround**: Use `snapshot --depth 3` on individual items to see the tree structure, then target the specific text node if needed. Or accept the mixed output and strip leading numeric lines.

## Pitfall 3: No batch/bulk text extraction

Reading N elements requires N sequential `get text` calls. Each call takes ~0.5s (app lookup + tree traversal + element resolution), so 48 items ≈ 30 seconds.

```bash
# Slow: 48 sequential calls
for i in $(seq 0 47); do
  axcli get --app Lark text ".feed-shortcut-item >> nth=$i"
done
```

**No parallel workaround**: axcli likely holds an accessibility API lock per call. Running multiple instances concurrently may cause race conditions.

**Mitigation**: Use `snapshot --all --depth 2` on the parent container to get all items in one call, then parse the tree output. Less clean but much faster.

## Pitfall 4: `nth=N` index invalidated by lazy loading / scroll

Lark uses lazy loading for long lists. After scrolling:
- Previously visible items may be destroyed
- New items loaded at different indices
- `nth=5` may now point to a completely different element

**Workaround**: Use `:has-text("keyword")` for content-based matching instead of positional indexing:
```bash
# Fragile after scroll
axcli click --app Lark '.feed-shortcut-item >> nth=5'

# Stable
axcli click --app Lark '.feed-shortcut-item:has-text("Security Team")'
```

## Pitfall 5: No structured output format

- `get text` returns plain text — no way to distinguish child element roles (name vs timestamp vs preview vs badge)
- `snapshot` returns indented tree text — useful for humans but painful to parse programmatically
- No `--json` output option for either command

**Impact**: Building reliable automation pipelines requires brittle text parsing. Any Lark UI update can break parsing assumptions.

## Pitfall 6: Elements outside viewport have wrong coordinates

`click`, `hover`, and `dblclick` use screen coordinates. Elements not currently visible in the scroll viewport report stale or zero coordinates.

**Workaround**: Always `scroll-to` before `click`/`hover`:
```bash
axcli scroll-to --app Lark '.feed-shortcut-item:has-text("Target Group")'
axcli click --app Lark '.feed-shortcut-item:has-text("Target Group")'
```

`snapshot` and `get text` are NOT affected — they read the accessibility tree directly, not screen positions.

## Recommended Workflow

1. **Screenshot** → understand current UI state visually
2. **Targeted snapshot** → inspect DOM structure of the area you need
3. **`get text`** → extract content (accept mixed badge/text)
4. **`:has-text()` locators** → interact with specific elements
5. **`scroll-to` before click** → ensure element is in viewport

Avoid: top-level snapshot, `nth=N` after scrolling, assuming `get text` output is clean.

---
name: record-pitfall
description: >
  Record development pitfalls and lessons learned after meaningful bug fixes.
  Use AFTER completing a non-trivial bug fix — especially when the fix required
  multiple attempts, revealed a non-obvious root cause, or uncovered framework/SDK
  behavior that would trip up future developers. Triggers on: bug fixed, pitfall,
  记录踩坑, lesson learned, debug method, 踩坑记录, record this fix, document this issue,
  write up what we learned. NOT for: trivial typo fixes, routine code changes,
  feature development without debugging.
---

# Record Pitfall

After completing a meaningful bug fix, write a structured pitfall document to `docs/pitfalls/` so the team avoids the same trap in the future.

## When to Record

Record a pitfall when ANY of these are true:

- The fix required **2+ failed attempts** before finding the right solution
- The root cause was **non-obvious** (not what you'd guess from the symptom)
- The issue involved **undocumented SDK/framework behavior**
- The debugging process revealed a **useful technique** worth sharing
- The fix involved a **subtle interaction** between components (e.g., one module's cleanup affecting another)

Do NOT record trivial fixes (typos, missing imports, obvious null checks).

## Document Structure

Create `docs/pitfalls/<descriptive-kebab-name>.md` with this structure:

```markdown
# <Title: concise description of the pitfall>

## Problem

What the user observed. Symptoms, not causes. Be specific — include error messages,
UI behavior, or log output that someone would see when hitting this issue.

## Root Cause

The actual technical reason. Explain WHY it happened, not just WHAT was wrong.
If the cause involves framework/SDK internals, explain the relevant mechanism.

## Approaches Tried (if multiple)

For each failed attempt:
- What was tried
- Why it seemed reasonable
- Why it didn't work (the specific reason, not just "didn't work")

This section is the most valuable part — it saves others from repeating the same
dead ends. Skip this section if the fix was found on the first attempt.

## Fix

The actual solution. Include code snippets if they clarify the fix.
Explain WHY this solution works, not just WHAT was changed.

## Lessons Learned

Generalizable takeaways. Think: "if someone hits a SIMILAR but not identical
problem, what principle from this experience would help them?"

Focus on:
- Mental models that were wrong and how to correct them
- Debugging techniques that proved effective
- SDK/framework behaviors to be aware of
- Architectural patterns to prefer or avoid
```

## File Naming

Use descriptive kebab-case names that someone could find via search:
- `react-native-keyboard-avoidance.md` (not `keyboard-fix.md`)
- `xmtp-sdk-stream-cancelled-by-verification.md` (not `stream-bug.md`)
- `zustand-selector-infinite-rerender.md` (not `rerender-fix.md`)

## Process

1. **Gather context** — read the conversation history to extract: symptoms, failed attempts, root cause, final fix, and debugging techniques used
2. **Write the document** — follow the structure above. Be concrete (include actual values, error messages, code). Avoid vague statements like "it didn't work properly"
3. **Also record to `docs/memory/`** — if the fix reveals a reusable pattern (e.g., "XMTP cancel methods are global"), write a shorter entry to `docs/memory/` focused on the actionable rule
4. **Commit** — stage and commit the pitfall doc with a descriptive commit message

## Quality Checklist

Before saving, verify:
- [ ] Someone unfamiliar with the codebase can understand the problem from the Problem section alone
- [ ] Root Cause explains the mechanism, not just "X was wrong"
- [ ] Failed approaches include WHY they failed (not just that they did)
- [ ] Fix section explains WHY it works
- [ ] Lessons are generalizable (useful beyond this exact bug)
- [ ] File name is searchable and descriptive

## Examples

See existing pitfall docs for reference:
- `docs/pitfalls/react-native-keyboard-avoidance.md` — 6 attempts, edge-to-edge root cause, detailed debugging method
- `docs/pitfalls/xmtp-sdk-stream-cancelled-by-verification.md` — non-obvious cancellation scope, dev bundle debug technique

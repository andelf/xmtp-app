# FlashList auto-scroll regression after bubble height changes

## Problem

After modifying MessageBubble layout (adding reply bars, reaction badges), new incoming messages no longer triggered auto-scroll to bottom.

## Root cause

The inverted FlashList uses `onScroll` to track whether the user is "at bottom" (offset < threshold). When bubble heights changed, FlashList would re-layout and emit scroll events with small non-zero offsets, falsely marking `isAtBottomRef.current = false`. The auto-scroll effect then skipped scrolling.

## Fix

1. Increase the "at bottom" threshold from 50px to 150px to tolerate layout reflow jitter
2. Add 50ms delay before `scrollToBottom()` to let FlashList finish layout

## Lesson

- Any change to list item height can break inverted FlashList scroll detection
- Use generous thresholds for "at bottom" detection in inverted lists
- Delay scroll commands to run after layout completes

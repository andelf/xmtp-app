# Mobile Foldable Layout Implementation Plan

Date: 2026-04-12

## Goal

Implement a React Native plan for foldable and large-screen Android devices that fixes the current chat layout bugs and adds a standard adaptive two-pane layout.

This plan intentionally covers only:

1. Phase 1: top/bottom inset and keyboard avoidance fixes
2. Phase 2: adaptive list-detail layout for expanded width

It does not include hinge/posture-aware native integration.

## Product goals

### Phase 1

- no overlap between the Android status bar and app UI
- no blank strip between the keyboard and the bottom composer
- normal phones must not regress

### Phase 2

- when the app window is wide enough, show:
  - left pane: conversation list
  - right pane: active conversation
- compact phones keep the current single-pane flow

## Scope constraints

This implementation must follow public Android and Samsung best practices, not custom foldable-specific hacks.

Do not do:

- device-name checks
- model-specific offsets
- hardcoded “Samsung Fold” layout branches
- one-off keyboard gap fixes with magic numbers
- custom navigation behavior unrelated to canonical list-detail

## Best-practice references

Android official:

- Window size classes:
  - https://developer.android.com/develop/ui/views/layout/window-size-classes
- Large-screen canonical layouts:
  - https://developer.android.com/guide/topics/large-screens/large-screen-canonical-layouts
- Responsive/adaptive design with Views:
  - https://developer.android.com/develop/ui/views/layout/responsive-adaptive-design-with-views
- Edge-to-edge:
  - https://developer.android.com/develop/ui/views/layout/edge-to-edge
  - https://developer.android.com/design/ui/mobile/guides/layout-and-content/edge-to-edge

Samsung official:

- App continuity:
  - https://developer.samsung.com/galaxy-z/app-continuity.html
- Foldable and large-screen continuity/multi-window:
  - https://developer.samsung.com/one-ui/foldable-and-largescreen/app-cont-and-multi.html
- Large-screen design:
  - https://developer.samsung.com/one-ui/largescreen-and-foldable/intro.html

## React Native interpretation of the guidance

These are Android best practices, but this app is React Native. So we translate them as follows:

- “Window size classes” means:
  - use `useWindowDimensions()` and width breakpoints in React Native
- “Canonical list-detail layout” means:
  - build a React Native master-detail screen shell
  - do not try to import Android `TwoPane` APIs directly
- “Insets are the source of truth” means:
  - use `react-native-safe-area-context` and keyboard/IME-aware layout behavior consistently
  - do not mix multiple bottom-compensation systems without clear ownership

This is a React Native implementation of Android large-screen guidance, not a native Android rewrite.

## Current codebase findings

Relevant files:

- [xmtp-mobile/app/_layout.tsx](/Users/mono/Repos/xmtp-app/xmtp-mobile/app/_layout.tsx:1)
- [xmtp-mobile/app/(main)/_layout.tsx](/Users/mono/Repos/xmtp-app/xmtp-mobile/app/(main)/_layout.tsx:1)
- [xmtp-mobile/app/(main)/conversations.tsx](/Users/mono/Repos/xmtp-app/xmtp-mobile/app/(main)/conversations.tsx:1)
- [xmtp-mobile/app/(main)/conversation/[id].tsx](/Users/mono/Repos/xmtp-app/xmtp-mobile/app/(main)/conversation/[id].tsx:1)
- [xmtp-mobile/src/components/MessageInput.tsx](/Users/mono/Repos/xmtp-app/xmtp-mobile/src/components/MessageInput.tsx:1)

Current layout issues:

- the conversation screen uses `KeyboardAvoidingView` from `react-native-keyboard-controller`
- it also uses `keyboardVerticalOffset={headerHeight}`
- it also manually switches bottom padding based on `keyboardVisible`
- top insets are not clearly owned by one layer
- layout is single-pane only

This is a classic source of:

- status-bar overlap on unusual window shapes
- doubled keyboard spacing
- stretched single-pane UI on large unfolded screens

## Phase 1: Fix insets and keyboard behavior on the existing single-pane UI

## Objective

Stabilize the existing chat screen on all Android form factors before introducing a dual-pane layout.

## Phase 1 implementation strategy

### 1. Define top inset ownership

We need one clear owner of top spacing for the conversation screen.

Implementation rule:

- either the stack header owns the top protected region
- or the screen content does
- but not both inconsistently

Work items:

- inspect current expo-router stack/header behavior on Android
- ensure the conversation content begins below the effective app bar/status bar region
- avoid layering extra top padding on top of already-inset header behavior unless explicitly required

### 2. Define bottom inset ownership

We need one clear owner of bottom spacing.

Current issue:

- keyboard translation
- header offset
- bottom safe-area padding
- keyboard visibility state

are all participating at once

Implementation rule:

- one layout path should own composer lifting above the IME
- safe-area bottom inset should be applied only where it is still needed
- do not combine keyboard translation and manual bottom padding in a way that can create a blank strip

### 3. Refactor the conversation screen layout

Target structure:

- top-level screen container
- message list area
- composer area

with explicit responsibilities:

- list fills remaining space
- composer sits at the bottom
- keyboard movement affects one owner path only

### 4. Preserve normal-phone behavior

This phase is not foldable-only. It should improve all Android form factors.

## Phase 1 expected code changes

- [conversation/[id].tsx](/Users/mono/Repos/xmtp-app/xmtp-mobile/app/(main)/conversation/[id].tsx:1)
- possibly [app/_layout.tsx](/Users/mono/Repos/xmtp-app/xmtp-mobile/app/_layout.tsx:1)
- possibly [app/(main)/_layout.tsx](/Users/mono/Repos/xmtp-app/xmtp-mobile/app/(main)/_layout.tsx:1)
- maybe one small reusable layout helper or hook for chat insets

## Phase 1 acceptance criteria

- no status-bar overlap on unfolded Samsung foldables
- no blank gap above the keyboard
- no new top/bottom spacing bugs on ordinary Android phones

## Phase 1 progress notes

The following issues have now been reproduced and fixed on the Samsung foldable debug device:

- top blank/overlap caused by double top inset application
- newest message drifting away from the bottom when keyboard is hidden
- bottom chat bubbles being covered when the keyboard opens

The concrete fixes were:

- remove the extra `paddingTop: insets.top` wrapper around `react-native-paper` `Appbar.Header`
- do not double-reserve composer height in the inverted message list
- when the keyboard is visible, add keyboard-lift space to the inverted `FlashList` via `ListHeaderComponent`

This means Phase 1 is now on the correct architecture:

- `Appbar.Header` owns top inset behavior
- `KeyboardStickyView` owns composer movement
- the inverted message list owns keyboard-time extra scroll range

## Phase 2: Adaptive list-detail layout for expanded width

## Objective

Use a canonical React Native list-detail layout on wide windows instead of stretching the current single-pane chat screen.

## Best-practice target

Follow Android’s canonical list-detail pattern and Samsung’s large-screen guidance:

- compact width:
  - keep current route-driven single-pane UX
- expanded width:
  - left pane conversation list
  - right pane active conversation

Initial pane ratio:

- `50:50` on foldables

## Phase 2 implementation strategy

### 1. Add a React Native width-class helper

Implementation rule:

- determine layout mode from current window width
- do not detect Samsung devices directly

Practical implementation:

- create a small `useWindowClass()` hook using `useWindowDimensions()`
- map width to:
  - compact
  - medium
  - expanded

Initial rollout:

- compact: single-pane
- expanded: two-pane
- medium: can stay single-pane in v1 unless testing shows it is clearly beneficial

### 2. Introduce an adaptive main shell

The current architecture is route-first. For expanded width, we need a shell that can render both panes together.

Recommended structure:

- adaptive authenticated shell
  - compact mode:
    - existing stack behavior
  - expanded mode:
    - conversations pane
    - conversation detail pane

### 3. Hoist selected-conversation state for expanded mode

Expanded list-detail requires shared selection state.

Implementation rule:

- selecting a conversation should update the right pane in place
- it should not behave like a full-screen push transition in expanded mode

### 4. Reuse existing screen content

Avoid creating a second, unrelated UI implementation.

Preferred direction:

- extract reusable conversation list content from the current conversations screen
- extract reusable conversation detail content from the current conversation screen
- compose them differently for compact vs expanded mode

This keeps the design system and message rendering logic consistent.

## Phase 2 expected code changes

- [xmtp-mobile/app/(main)/_layout.tsx](/Users/mono/Repos/xmtp-app/xmtp-mobile/app/(main)/_layout.tsx:1)
- [xmtp-mobile/app/(main)/conversations.tsx](/Users/mono/Repos/xmtp-app/xmtp-mobile/app/(main)/conversations.tsx:1)
- [xmtp-mobile/app/(main)/conversation/[id].tsx](/Users/mono/Repos/xmtp-app/xmtp-mobile/app/(main)/conversation/[id].tsx:1)
- likely new shared components, for example:
  - `src/components/adaptive/MainAdaptiveLayout.tsx`
  - `src/components/conversations/ConversationListPane.tsx`
  - `src/components/conversations/ConversationDetailPane.tsx`
  - `src/hooks/useWindowClass.ts`

## Phase 2 acceptance criteria

- unfolded foldable in expanded width shows list + detail side by side
- selecting a conversation updates the right pane in place
- compact phones remain on the current single-pane navigation flow
- the expanded layout does not regress keyboard behavior from Phase 1

## Validation plan

Required checks:

1. normal phone portrait
2. normal phone landscape
3. foldable cover screen
4. foldable unfolded full-screen
5. foldable unfolded with keyboard open
6. split-screen if available

## Recommended execution order

1. Phase 1
2. device validation on regular phone + foldable
3. Phase 2
4. device validation again

## Recommendation

Proceed exactly in this order:

1. fix insets and keyboard handling on the current single-pane chat UI
2. add adaptive list-detail layout for expanded width

That keeps the implementation aligned with official Android/Samsung best practices and avoids inventing a custom foldable UI system.

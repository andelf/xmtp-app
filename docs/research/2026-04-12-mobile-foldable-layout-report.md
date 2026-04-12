# Mobile Foldable Layout Research

Date: 2026-04-12

## Scope

This report analyzes the current `xmtp-mobile` layout issues observed on Samsung foldable devices, especially when the device is fully unfolded and the app window becomes close to square.

User-reported issues:

1. The Android status bar area overlaps the app's top content.
2. When the keyboard opens, there is a blank gap between the keyboard top edge and the app's bottom input area.
3. In the unfolded large-screen state, the single-pane chat UI looks stretched and inefficient; a Telegram-style master-detail layout is preferred:
   - left pane: conversation list
   - right pane: active conversation

This report answers:

1. Whether these issues are real and explainable from the current codebase
2. Whether they can be fixed
3. What implementation strategy is appropriate for this app

## Current codebase findings

Relevant files:

- [xmtp-mobile/app/_layout.tsx](/Users/mono/Repos/xmtp-app/xmtp-mobile/app/_layout.tsx:1)
- [xmtp-mobile/app/(main)/_layout.tsx](/Users/mono/Repos/xmtp-app/xmtp-mobile/app/(main)/_layout.tsx:1)
- [xmtp-mobile/app/(main)/conversation/[id].tsx](/Users/mono/Repos/xmtp-app/xmtp-mobile/app/(main)/conversation/[id].tsx:1)
- [xmtp-mobile/src/components/MessageInput.tsx](/Users/mono/Repos/xmtp-app/xmtp-mobile/src/components/MessageInput.tsx:1)

Observed current behavior in code:

- The app uses `SafeAreaProvider`, but the chat screen itself does not explicitly inset top content for the status bar.
- The chat screen uses `expo-router` stack header plus `react-native-keyboard-controller` `KeyboardAvoidingView` with `behavior="translate-with-padding"` and `keyboardVerticalOffset={headerHeight}`.
- The bottom input wrapper manually applies `paddingBottom: keyboardVisible ? 0 : insets.bottom`.
- The layout is single-pane only; there is no window-size-based adaptive branch, no `useWindowDimensions`, and no fold/hinge awareness.

That means the current chat screen is optimized for a normal slab-phone portrait layout and is not intentionally designed for:

- large unfolded foldable windows
- square-ish windows
- half-open fold postures
- multi-window resizing

## What official guidance says

### 1. Foldables and large screens must be treated as adaptive layouts

Android's current adaptive quality guidance explicitly treats foldables, posture changes, and multi-window as first-class scenarios. It recommends testing at least on a foldable window around `841x701 dp`.

Source:

- Android Adaptive app quality guidelines:
  - https://developer.android.com/docs/quality-guidelines/adaptive-app-quality

Key takeaways:

- Apps should not rely on a phone-only portrait layout assumption.
- Apps should handle form factor, posture, and window resizing without overlap or degraded UX.

### 2. Samsung expects continuity across cover screen, inner screen, and resizing

Samsung's foldable guidance emphasizes:

- seamless continuity when folding/unfolding
- preserving scroll position and text-entry state
- supporting resizable multi-window layouts

Source:

- Samsung App continuity:
  - https://developer.samsung.com/galaxy-z/app-continuity.html
- Samsung foldable design guidance:
  - https://developer.samsung.com/one-ui/foldable-and-largescreen/app-cont-and-multi.html

Important Samsung checkpoint directly relevant here:

- if the user was entering text, text input and keyboard state should remain consistent across screen changes

This aligns with the draft-preservation work we already added.

### 3. Edge-to-edge means status bar overlap must be handled with insets, not guesswork

Android's edge-to-edge guidance is explicit:

- backgrounds can draw behind system bars
- tappable or important UI must react to system bar insets
- top bars and critical top content must avoid visual overlap with status bar and cutouts

Sources:

- Android Views edge-to-edge:
  - https://developer.android.com/develop/ui/views/layout/edge-to-edge
- Android design edge-to-edge:
  - https://developer.android.com/design/ui/mobile/guides/layout-and-content/edge-to-edge

This strongly suggests the current top overlap is not a Samsung-only quirk; it is a layout/insets bug.

### 4. Fold posture and hinge information should be available to layout code

Android recommends using Jetpack WindowManager to detect:

- `FoldingFeature`
- `state`
- `orientation`
- `isSeparating`
- `occlusionType`

Source:

- Make your app fold aware:
  - https://developer.android.google.cn/develop/ui/compose/layouts/adaptive/foldables/make-your-app-fold-aware?hl=en

This is especially important for:

- tabletop posture
- book posture
- avoiding placing controls or text across the fold/hinge

### 5. Samsung explicitly recommends multi-pane layouts for unfolded large screens

Samsung's large-screen guidance recommends multi-pane layouts on large screens and specifically recommends `50:50` for foldables.

Source:

- Samsung large screen design:
  - https://developer.samsung.com/one-ui/largescreen-and-foldable/intro.html

This directly supports the Telegram-style conversation-list + conversation-detail layout proposal.

## Analysis of the three reported problems

### Problem A: status bar overlaps top app content

#### Symptom

On unfolded foldables, the top area containing signal/battery/time overlaps with the app's top portion.

#### Likely cause in current code

The chat page relies on the stack header, but the screen content itself is not designed around explicit top insets. On foldables and large-screen edge-to-edge windows, the effective top inset and app bar geometry can differ enough that the current layout becomes fragile.

Relevant code:

- [conversation/[id].tsx](/Users/mono/Repos/xmtp-app/xmtp-mobile/app/(main)/conversation/[id].tsx:280)
- [app/_layout.tsx](/Users/mono/Repos/xmtp-app/xmtp-mobile/app/_layout.tsx:34)

#### Feasibility

Yes, definitely fixable.

#### Correct direction

- treat header/background and content insets separately
- explicitly apply status bar and display cutout protections
- verify whether the expo-router stack header itself is already consuming insets, then make the content area avoid double- or under-compensation

#### Risk

Low to medium. Mostly a layout cleanup task.

### Problem B: blank gap between input bar and keyboard

#### Symptom

When the keyboard opens, there is a visible empty strip between the keyboard and the input area.

#### Likely cause in current code

The chat screen is currently mixing multiple offset systems:

- `KeyboardAvoidingView` with `translate-with-padding`
- `keyboardVerticalOffset={headerHeight}`
- manual bottom padding via `insets.bottom` when keyboard is closed
- `keyboardVisible` boolean derived from keyboard events

Relevant code:

- [conversation/[id].tsx](/Users/mono/Repos/xmtp-app/xmtp-mobile/app/(main)/conversation/[id].tsx:106)
- [conversation/[id].tsx](/Users/mono/Repos/xmtp-app/xmtp-mobile/app/(main)/conversation/[id].tsx:305)
- [conversation/[id].tsx](/Users/mono/Repos/xmtp-app/xmtp-mobile/app/(main)/conversation/[id].tsx:333)

This creates a strong possibility of double-applying bottom avoidance on foldables, especially when:

- the app window height changes significantly
- the unfolded window is close to square
- the keyboard consumes a larger fraction of the height

#### Feasibility

Yes, fixable.

#### Correct direction

Unify around one source of truth for bottom avoidance:

- use real IME insets instead of mixing keyboard events and manual padding heuristics
- separate:
  - system bottom inset
  - keyboard inset
  - input bar intrinsic height
- avoid additive compensation that can create a gap

Pragmatically, this probably means refactoring the chat screen away from the current "manual `keyboardVisible` + bottom safe-area padding + translated wrapper" combination.

#### Risk

Medium. This is the most likely area to regress ordinary phones if changed carelessly, so it should be fixed with foldable and regular-phone verification together.

### Problem C: unfolded UI should become two-pane instead of stretched single-pane

#### Symptom

When the screen becomes near-square or large-width, a single chat pane wastes space and looks awkward.

#### Feasibility

Yes, feasible, and aligned with Samsung and Android guidance.

#### Recommended target behavior

For sufficiently large unfolded widths:

- left pane: conversation list
- right pane: active conversation
- keep conversation detail and navigation in a master-detail arrangement

For compact widths:

- keep today's single-pane navigation flow

#### Why this is reasonable

Samsung explicitly recommends multi-pane large-screen layouts, including a `50:50` layout for foldables. Android also recommends adaptive layouts by window size class.

#### Likely implementation direction

Phase 1:

- detect width class with `useWindowDimensions()`
- switch to dual-pane when width is above a chosen breakpoint and the app is not in a narrow cover-screen state

Phase 2:

- optionally add fold awareness through Jetpack WindowManager for:
  - hinge separation
  - tabletop / book posture behavior
  - not placing important content across fold bounds

For our React Native app, the realistic first implementation is width-class driven, not full hinge-aware native posture logic on day one.

#### Risk

Medium to high, because this is a structural navigation/layout change rather than a small padding fix.

## Can this be implemented in the current React Native app?

Yes.

But the three items should not be treated as one single patch:

### Straightforward now

- preserving per-conversation draft text: already done
- top overlap fix via safer inset handling
- keyboard-gap fix via unified IME/system inset handling

### Feasible but should be a separate feature branch

- adaptive two-pane foldable layout

The two-pane layout is feasible without rewriting the app, but it touches:

- navigation structure
- conversations list screen
- conversation detail screen
- state ownership for selected conversation
- tablet/foldable-specific back behavior

## Recommended implementation plan

### Phase 1: fix top and bottom inset bugs

Goal:

- no status-bar overlap
- no keyboard gap
- no regression on regular phones

Suggested tasks:

1. Audit actual inset ownership between stack header and screen content.
2. Replace keyboard-gap heuristics with a single IME-aware bottom layout path.
3. Test on:
   - normal phone portrait
   - unfolded foldable portrait
   - unfolded foldable landscape
   - split-screen if possible

### Phase 2: add adaptive dual-pane layout

Goal:

- compact width: current single-pane flow
- medium/expanded width: list + detail side by side

Suggested breakpoints:

- compact: width < 600 dp
- medium and above: consider dual-pane

Samsung-specific note:

- on foldables, `50:50` is a sensible first ratio

### Phase 3: optional fold-awareness

Goal:

- avoid hinge/fold conflicts
- support tabletop/book posture more intentionally

This likely requires native Android integration through Jetpack WindowManager, then exposing posture/window feature info to React Native.

## Practical recommendation for this repo

The best near-term path is:

1. Fix the chat screen insets first
2. Then add dual-pane adaptive layout
3. Only then decide whether hinge/posture awareness is worth the added native complexity

Reason:

- The first two user-visible issues are almost certainly caused by current layout/inset handling, not by lack of fold posture APIs.
- A width-based dual-pane layout will already improve unfolded Z Fold UX significantly.
- Full hinge-aware behavior is valuable, but not necessary to get a major UX improvement.

## Proposed acceptance criteria

### Inset fixes

- No content overlaps the status bar on unfolded Samsung foldables
- No visible blank strip appears between keyboard and input bar
- Chat list and input remain usable in portrait, landscape, and unfolded square-ish windows

### Dual-pane layout

- On large unfolded width, show conversation list and active conversation side by side
- Switching conversations updates the right pane without a full-screen route transition
- Compact phones keep the current single-pane UX

## Conclusion

All three requested improvements are implementable.

My judgment:

- top overlap: definitely worth fixing now
- keyboard gap: definitely worth fixing now
- Telegram-style dual-pane foldable UI: feasible and aligned with both Android and Samsung guidance, but should be done as a dedicated adaptive-layout feature rather than bundled into the inset bugfix

## Sources

- Android Adaptive app quality guidelines:
  - https://developer.android.com/docs/quality-guidelines/adaptive-app-quality
- Android edge-to-edge in Views:
  - https://developer.android.com/develop/ui/views/layout/edge-to-edge
- Android edge-to-edge design:
  - https://developer.android.com/design/ui/mobile/guides/layout-and-content/edge-to-edge
- Android fold-aware guidance:
  - https://developer.android.google.cn/develop/ui/compose/layouts/adaptive/foldables/make-your-app-fold-aware?hl=en
- Samsung App continuity:
  - https://developer.samsung.com/galaxy-z/app-continuity.html
- Samsung foldable continuity and multitasking:
  - https://developer.samsung.com/one-ui/foldable-and-largescreen/app-cont-and-multi.html
- Samsung large-screen design:
  - https://developer.samsung.com/one-ui/largescreen-and-foldable/intro.html

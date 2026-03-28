# React Native Keyboard Avoidance on Android (Expo SDK 55 + Edge-to-Edge)

## Background

Chat screen layout: `Header + Inverted FlashList + Input Bar (bottom)`

Target behavior:
- Keyboard closed: input bar sits above Android navigation bar (3-button nav, 44dp)
- Keyboard open: input bar sticks to keyboard top, message list shrinks, newest messages visible

Environment: Expo SDK 55, Android physical device, 3-button navigation (insets.bottom=44dp), keyboard height ~291dp.

## The Root Cause

Expo SDK 53+ enables **edge-to-edge** mode by default on Android. This fundamentally breaks the traditional `adjustResize` + `KeyboardAvoidingView` approach:

- `android:windowSoftInputMode="adjustResize"` no longer resizes the window when edge-to-edge is active
- The keyboard height (291dp) **includes** the navigation bar area (44dp)
- Standard React Native `KeyboardAvoidingView` has no awareness of this overlap

## Approaches Tried (Chronological)

### Attempt 1: RN Built-in KeyboardAvoidingView + behavior="height"

```tsx
<KeyboardAvoidingView behavior={Platform.OS === "ios" ? "padding" : "height"}>
  <FlashList inverted ... />
  <MessageInput />
</KeyboardAvoidingView>
```

**Result**: Completely ineffective. Input bar did not move at all. Edge-to-edge mode means `adjustResize` is a no-op, so `KeyboardAvoidingView` has nothing to react to.

### Attempt 2: Manual BottomSpacer with Keyboard Event Listeners

Listened to `keyboardDidShow`/`keyboardDidHide`, got keyboard height, calculated spacer:

```tsx
function BottomSpacer() {
  const kbHeight = useKeyboardHeight(); // from keyboardDidShow event
  const insets = useSafeAreaInsets();
  const h = kbHeight > 0 ? kbHeight - insets.bottom : insets.bottom;
  return <View style={{ height: Math.max(h, 0) }} />;
}
```

**Result**: Input bar moved but was still partially covered by keyboard. The `kbHeight - insets.bottom` calculation was correct in theory but the spacer was a sibling below the input bar in a flex column -- it pushed content down but nothing lifted the container up.

**Key Insight**: A bottom spacer in a flex column cannot lift the column itself. It only adds space at the bottom, which is invisible when the keyboard covers it.

### Attempt 3: KeyboardStickyView (react-native-keyboard-controller)

Installed `react-native-keyboard-controller`. Used `KeyboardStickyView` to make the input bar translate with the keyboard animation:

```tsx
<KeyboardStickyView offset={{ closed: insets.bottom, opened: 0 }}>
  <MessageInput />
</KeyboardStickyView>
```

**Result**: Input bar correctly tracked the keyboard animation. However, the FlashList container height remained unchanged -- the input bar floated over the list via CSS translate, but the list didn't know to shrink. Newest messages were hidden behind the translated input bar.

**Key Insight**: `KeyboardStickyView` uses **translation** (not layout resize). It moves the component visually but doesn't affect the flex layout. The message list's available height stays the same.

### Attempt 4: KeyboardStickyView + Dynamic paddingBottom on FlashList

Tried adding keyboard-height-dependent padding to FlashList's contentContainerStyle:

```tsx
contentContainerStyle={{
  paddingBottom: isKbVisible ? kbHeight : insets.bottom + 70,
}}
```

**Result**: Padding changed but the inverted FlashList didn't auto-scroll to show the newest messages. The content area effectively had the right size, but the scroll position wasn't adjusted.

### Attempt 5: KeyboardAvoidingView from react-native-keyboard-controller + behavior="padding"

```tsx
<KBAvoidingView style={styles.container} behavior="padding">
  <FlashList ... />
  <MessageInput />
</KBAvoidingView>
```

**Result**: Input bar disappeared completely -- pushed below the screen. The `behavior="padding"` added paddingBottom equal to the full keyboard height, which combined with the existing layout pushed everything out of view.

### Attempt 6 (Final): behavior="translate-with-padding" + Conditional Bottom Padding

**How we found this**: After 5 failed attempts, we stopped guessing and delegated to a research subagent with explicit instructions to search for:
- Official `react-native-keyboard-controller` chat screen examples
- The library's GitHub example source code
- Community solutions for Expo SDK 53+ edge-to-edge keyboard issues

The research found that `react-native-keyboard-controller` has a **chat-specific** behavior value: `"translate-with-padding"`. This was not obvious from the library's main documentation page -- it was found in:
1. The library's GitHub example `ReanimatedChatFlatList`
2. The components overview guide that explains behavior differences

```tsx
// Root layout: KeyboardProvider with translucent flags
<KeyboardProvider statusBarTranslucent navigationBarTranslucent>
  {children}
</KeyboardProvider>

// Chat screen:
<KeyboardAvoidingView
  behavior="translate-with-padding"
  keyboardVerticalOffset={headerHeight}  // from useHeaderHeight()
  style={{ flex: 1 }}
>
  <FlashList inverted ... />
  <View style={{ paddingBottom: keyboardVisible ? 0 : insets.bottom }}>
    <MessageInput />
  </View>
</KeyboardAvoidingView>
```

**Result**: Correct behavior on all counts:
- Keyboard closed: input bar above navigation bar (paddingBottom = insets.bottom = 44dp)
- Keyboard open: entire container translates up + padding adjusts, input bar touches keyboard top
- FlashList area shrinks naturally, newest messages stay visible

## Why "translate-with-padding" Works

| behavior | What it does | Good for |
|----------|-------------|----------|
| `padding` | Adds paddingBottom to container | Forms, ScrollViews |
| `height` | Shrinks container height | General layouts |
| `position` | Translates container up | Fixed bottom buttons |
| `translate-with-padding` | Translates container up + adds paddingTop once at animation start | **Chat screens with inverted lists** |

The key difference: `translate-with-padding` moves the entire view (so the input bar goes up) while simultaneously adjusting the layout so the list knows to display fewer items. Regular `padding` only adjusts layout without moving, and regular `position` only moves without adjusting layout.

## Additional Fix: Conditional Bottom Safe Area

The navigation bar spacer (44dp) must be hidden when the keyboard is open, because the keyboard already occupies that space:

```tsx
const isKeyboardVisible = useKeyboardState((s) => s.isVisible);

<View style={{ paddingBottom: isKeyboardVisible ? 0 : insets.bottom }}>
  <MessageInput />
</View>
```

Without this, the input bar floats 44dp above the keyboard when it's open.

## Critical Configuration

### KeyboardProvider Props

```tsx
<KeyboardProvider statusBarTranslucent navigationBarTranslucent>
```

Both `statusBarTranslucent` and `navigationBarTranslucent` are **required** in edge-to-edge mode. Without them, the library adds its own padding compensation that conflicts with `useSafeAreaInsets()`, causing a double-padding bug (documented in the library's GitHub discussion #984).

### Buffer Polyfill

`@xmtp/react-native-sdk` internally uses `Buffer.from().toString('base64')` in its signature handling code. Hermes (React Native's JS engine) does not have a global `Buffer`. This manifests as `"ERROR in create. User rejected signature"` with the real error being `ReferenceError: Property 'Buffer' doesn't exist`.

Fix in root layout:
```tsx
import { Buffer } from "buffer";
if (typeof globalThis.Buffer === "undefined") {
  globalThis.Buffer = Buffer as any;
}
```

## Thinking Process That Led to the Solution

1. **Attempts 1-5 were "guess and check"** -- trying different combinations of known React Native keyboard APIs without understanding the fundamental change that edge-to-edge introduced.

2. **The breakthrough was stopping manual attempts and searching for authoritative sources** -- specifically the library's own example code and GitHub issues. The `"translate-with-padding"` behavior is not prominently documented but exists in the example app.

3. **Decomposing the problem into two independent sub-problems** helped:
   - Sub-problem A: How to move the input bar (translation)
   - Sub-problem B: How to shrink the list area (layout adjustment)
   - Previous attempts solved one or the other but not both. `translate-with-padding` is the only behavior that does both simultaneously.

4. **Using device logs to get exact measurements** (keyboard=291dp, nav=44dp, status=40dp) made it possible to verify each attempt's behavior precisely rather than guessing from visual observation.

## Device Data Reference

| Measurement | Value | Source |
|-------------|-------|--------|
| Keyboard height | 290.67dp | `keyboardDidShow` event `endCoordinates.height` |
| Navigation bar (insets.bottom) | 44dp | `useSafeAreaInsets()` |
| Status bar (insets.top) | 40dp | `useSafeAreaInsets()` |
| Header height | varies | `useHeaderHeight()` from `@react-navigation/elements` |

## Dependencies

```
react-native-keyboard-controller: ^1.20.7
react-native-reanimated: ^4.2.1
react-native-worklets: 0.7.1  (must match reanimated's requirement, NOT 0.8.x)
react-native-safe-area-context: (bundled with Expo)
buffer: ^6.x  (polyfill for XMTP SDK)
```

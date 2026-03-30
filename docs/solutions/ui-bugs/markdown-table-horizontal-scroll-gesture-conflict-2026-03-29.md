---
title: "Markdown table horizontal scroll blocked by React Native gesture system on Android"
date: 2026-03-29
category: ui-bugs
module: xmtp-mobile
problem_type: ui_bug
component: frontend_stimulus
symptoms:
  - "Wide markdown tables clipped at right edge with no horizontal scrolling"
  - "adb logcat shows ACTION_DOWN -> ACTION_MOVE -> ACTION_CANCEL on table touch"
  - "Scrollbar appears but table content does not scroll"
root_cause: async_timing
resolution_type: code_fix
severity: medium
tags:
  - react-native
  - android
  - gesture-conflict
  - horizontal-scroll
  - markdown
  - patch-package
  - native-module
---

# Markdown table horizontal scroll blocked by React Native gesture system on Android

## Problem

Markdown tables rendered by `react-native-enriched-markdown` (v0.4.1) in the XMTP React Native Android app were not horizontally scrollable, despite the library already wrapping tables in a native `HorizontalScrollView`. Wide tables were clipped at the screen edge with overflowed content invisible and unreachable.

## Symptoms

- Wide markdown tables clipped at the right edge of the screen with no way to see overflowed content
- `adb logcat` with debug logging showed every touch sequence as `ACTION_DOWN -> ACTION_MOVE -> ACTION_CANCEL` — the parent was stealing the gesture
- The scrollbar indicator appeared (library correctly detected `totalTableWidth > viewWidth`), but actual scrolling never occurred

## What Didn't Work

- **First rebuild after adding debug logs**: Gradle's incremental build skipped Kotlin recompilation for the library module. The new logging code never ran. Fix was to delete the specific module's `android/build/` directory and rebuild.
- **`./gradlew clean` (full project clean)**: Deleted CMake-generated native libraries for other dependencies, causing a full rebuild failure. The correct approach is to clean only the target module's build cache.
- **First `patch-package` attempt**: Run without cleaning `android/build/` first, so the generated patch included 3771 lines of build artifacts (542KB). Had to delete the build directory and regenerate a clean patch.

## Solution

Override `onInterceptTouchEvent` in a custom `HorizontalScrollView` subclass within `TableContainerView.kt` to manage gesture priority.

**Before** (anonymous `HorizontalScrollView`):

```kotlin
private val scrollView = HorizontalScrollView(context).apply {
    isHorizontalScrollBarEnabled = true
    overScrollMode = View.OVER_SCROLL_NEVER
    addView(GridContainerView(context))
}
```

**After** (custom subclass with gesture arbitration):

```kotlin
private val scrollView =
    object : HorizontalScrollView(context) {
      private var startX = 0f
      private var startY = 0f
      private var didDisallow = false

      override fun onInterceptTouchEvent(ev: MotionEvent): Boolean {
        when (ev.action) {
          MotionEvent.ACTION_DOWN -> {
            startX = ev.x
            startY = ev.y
            didDisallow = false
            parent?.requestDisallowInterceptTouchEvent(true)
          }
          MotionEvent.ACTION_MOVE -> {
            val dx = Math.abs(ev.x - startX)
            val dy = Math.abs(ev.y - startY)
            if (!didDisallow && dx > dy && dx > 8f * density) {
              didDisallow = true
              parent?.requestDisallowInterceptTouchEvent(true)
            } else if (!didDisallow && dy > dx && dy > 8f * density) {
              parent?.requestDisallowInterceptTouchEvent(false)
            }
          }
          MotionEvent.ACTION_UP, MotionEvent.ACTION_CANCEL -> {
            parent?.requestDisallowInterceptTouchEvent(false)
            didDisallow = false
          }
        }
        return super.onInterceptTouchEvent(ev)
      }

      init {
        isHorizontalScrollBarEnabled = true
        overScrollMode = View.OVER_SCROLL_NEVER
        addView(GridContainerView(context))
      }
    }
```

Fix persisted via `patch-package` at `xmtp-mobile/patches/react-native-enriched-markdown+0.4.1.patch`.

## Why This Works

Android's touch event system is parent-wins-by-default. When `FlatList` (`RecyclerView`) or `Pressable` (long-press) detects motion, it calls `onInterceptTouchEvent` and claims the gesture. The child `HorizontalScrollView` receives `ACTION_CANCEL` and never scrolls.

`requestDisallowInterceptTouchEvent(true)` is the Android API for this scenario. On `ACTION_DOWN`, the child preemptively tells ancestors "do not intercept." On `ACTION_MOVE`, it measures gesture direction:

- **Horizontal drag** (`dx > dy`, beyond 8dp dead zone): Keeps disallow — table scrolls horizontally
- **Vertical drag** (`dy > dx`): Releases disallow — parent FlatList scrolls vertically

This gives the child first right of refusal on gesture direction. (auto memory [claude]: diagnosed via adb logcat before attempting fixes)

## Prevention

- **Test nested scrollable views on Android specifically.** iOS handles gesture arbitration more permissively. A table that scrolls on iOS may be broken on Android.
- **When embedding scrollable content inside RN lists**, assume the parent will steal gestures. Native scrollable children need explicit `requestDisallowInterceptTouchEvent` handling on Android.
- **Use `adb logcat` with targeted logging as the first diagnostic step.** The `ACTION_CANCEL` pattern immediately identified the root cause.
- **When patching `node_modules` with `patch-package`**, always delete the module's `android/build/` directory before generating the patch.
- **When modifying Kotlin in `node_modules`**, delete that module's build cache before rebuilding. Gradle incremental build may skip recompilation silently.

## Related Issues

- [docs/adr/004-markdown-rendering-native-component.md](../../adr/004-markdown-rendering-native-component.md) — ADR noting "Horizontal ScrollView around the native component doesn't work"; this fix resolves that consequence
- [docs/solutions/ui-bugs/android-oem-monospace-font-not-applied-2026-03-29.md](android-oem-monospace-font-not-applied-2026-03-29.md) — Sibling fix in the same patch file (different problem: OEM monospace font)
- [docs/pitfalls/react-native-keyboard-avoidance.md](../../pitfalls/react-native-keyboard-avoidance.md) — Related Android touch/gesture conflict pattern (keyboard vs. layout)

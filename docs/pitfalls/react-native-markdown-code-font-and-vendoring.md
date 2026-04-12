# React Native Markdown: Android code block font regression and vendored renderer pitfalls

## Read This First

If you are about to change any of these:

- `xmtp-mobile/src/components/MessageBubble.tsx`
- `xmtp-mobile/metro.config.js`
- `vendor/react-native-enriched-markdown/**`
- `.github/workflows/mobile.yml`

stop and read this file first.

This regression looked simple and was not simple. The trap is that inline code and block code do not use the same Android rendering path.

## The Actual Rendering Split

On Android in the vendored markdown package:

- inline code uses `spans/CodeSpan.kt`
- block code uses `views/CodeBlockContainerView.kt`

This distinction is the whole problem.

If you only patch `CodeSpan.kt`, inline code may look correct while fenced code blocks still render with the wrong font.

If you only stare at `MessageBubble.tsx`, you can also miss the real bug, because the JS style reaches two different native implementations.

## The Real Root Cause

The stable build was failing specifically for fenced code blocks because `CodeBlockContainerView.kt` treated `fontFamily: "monospace"` like a generic font family string:

- it called `SpanStyleCache.getTypeface(codeStyle.fontFamily, Typeface.NORMAL)`
- that path did not reliably use the bundled JetBrains Mono asset font
- inline code was using a different native path with better monospace fallback behavior

Result:

- inline code could look correct
- code blocks could still look proportional
- changing nearby size or font settings in `MessageBubble.tsx` made it look random, even though the actual bug was in the block renderer

There was a second bug mixed into the same debugging session:

- long inline code and long link text on Android were not getting usable soft wrap opportunities
- React Native text layout therefore treated them as effectively harder-to-break runs
- in real message bubbles, that could interact with list/paragraph spacing and async measurement in a way that left the final rendered text one or two lines taller than the bubble that was first allocated

This is why the visual symptom sometimes looked like "random extra blank lines" or "the last line is hanging outside the bubble" even after the monospace fix was correct.

## The Correct Fix

`CodeBlockContainerView.kt` must treat `"monospace"` as a special case and route it to the asset-backed monospace fallback:

- initialize bundled fonts with `SpanStyleCache.initAssetFonts(context)`
- if `fontFamily` is empty or exactly `"monospace"`, use `SpanStyleCache.getMonospaceTypeface(Typeface.NORMAL)`
- only use `SpanStyleCache.getTypeface(...)` for non-monospace custom font families

This same rule must be used both for:

- runtime rendering in `resolveMonospaceTypeface()`
- height measurement in `measureCodeBlockNodeHeight(...)`

If you update one and not the other, rendering and measurement can drift.

## Important: What Was Wrong Before

### Wrong assumption 1: block code uses `CodeBlockSpan.kt`

It does not, at least not for the horizontally scrollable fenced block path we care about.

The working path is `CodeBlockContainerView.kt`.

### Wrong assumption 2: changing `fontSize` broke monospace

Not directly.

The regression appeared while changing size because adjacent edits also changed the Android font path. That made it look like a size-only regression, but the actual dependency was the typeface selection path.

### Wrong assumption 3: leaving Android `fontFamily` empty is the whole fix

That was an incomplete mental model.

The real rule is:

- understand which native renderer the markdown node uses
- make sure that renderer maps code blocks to the JetBrains Mono fallback path

JS-side empty string versus `"monospace"` is only safe if the native code handles it correctly.

## Stable Rules Going Forward

When touching mobile markdown code fonts:

1. Always inspect both `CodeSpan.kt` and `CodeBlockContainerView.kt`.
2. Treat inline code and code block as separate Android code paths until proven otherwise.
3. Do not assume a JS-only style tweak fixes a native font regression.
4. If Android code block font regresses, check `CodeBlockContainerView.kt` before touching `MessageBubble.tsx`.
5. If you want smaller code blocks, prefer changing only one visual variable at a time.
6. After every font-related change, rebuild a release APK and verify on-device.

When touching wrapping / bubble height on Android:

1. Check whether the problematic content is actually a long inline run such as inline code or a link.
2. Remember that fixing `requestLayout()` alone may reduce symptoms without fixing the underlying wrapping problem.
3. If the broken message becomes correct after adding soft wrap opportunities, the root cause was not "height math only".
4. For long inline code and links, prefer inserting zero-width soft break opportunities in the rendered Android text path so measurement and display stay aligned.

## Verified Safe Setup

Current safe setup is:

- `MessageBubble.tsx`
  - iOS uses `Menlo`
  - Android passes `"monospace"` for markdown `code` and `codeBlock`
- `vendor/react-native-enriched-markdown/android/src/main/java/com/swmansion/enriched/markdown/spans/CodeSpan.kt`
  - inline code prefers bundled JetBrains Mono asset fonts
- `vendor/react-native-enriched-markdown/android/src/main/java/com/swmansion/enriched/markdown/views/CodeBlockContainerView.kt`
  - block code initializes asset fonts and maps `"monospace"` to `SpanStyleCache.getMonospaceTypeface(...)`
- `vendor/react-native-enriched-markdown/android/src/main/java/com/swmansion/enriched/markdown/renderer/CodeRenderer.kt`
  - long inline code inserts zero-width break opportunities so Android can wrap instead of overflowing the bubble
- `vendor/react-native-enriched-markdown/android/src/main/java/com/swmansion/enriched/markdown/renderer/LinkRenderer.kt`
  - long link text inserts zero-width break opportunities while preserving spans, so wrapped height matches rendered height more closely
- `vendor/react-native-enriched-markdown/android/src/main/java/com/swmansion/enriched/markdown/renderer/SpanStyleCache.kt`
  - loads and caches JetBrains Mono assets from APK fonts

## Metro / Vendoring Pitfall

This font problem was separate from the vendoring problem, but both showed up around the same time.

After switching `react-native-enriched-markdown` to:

```json
"react-native-enriched-markdown": "file:../vendor/react-native-enriched-markdown"
```

Expo/Metro required explicit symlink support. Keep `xmtp-mobile/metro.config.js` with:

- `watchFolders` including `../vendor/react-native-enriched-markdown`
- `resolver.unstable_enableSymlinks = true`
- `resolver.nodeModulesPaths` covering both app and vendored package

Do not mix this up with the Android font bug. They are different failures.

## CI Pitfall

GitHub Actions can fail on vendored dependency resolution even if local Android builds pass.

That is a CI/package-resolution problem, not evidence that the font fix is wrong.

Check separately:

- `xmtp-mobile/package.json`
- `xmtp-mobile/package-lock.json`
- `.github/workflows/mobile.yml`

## Debug Checklist

If code block monospace disappears again:

1. Confirm whether inline code is correct and fenced code block is wrong, or both are wrong.
2. Read `MessageBubble.tsx` and verify the current `code` / `codeBlock` `fontFamily`.
3. Read `CodeSpan.kt`.
4. Read `CodeBlockContainerView.kt`.
5. Verify `CodeBlockContainerView.kt` still maps `"monospace"` to `getMonospaceTypeface(...)`.
6. Verify JetBrains Mono fonts are still bundled in the APK.
7. Rebuild a release APK and test on device before drawing conclusions.

If a markdown bubble still overflows after the font fix:

1. Check whether the overflowing line is inside inline code or link text.
2. Read `CodeRenderer.kt` and `LinkRenderer.kt`.
3. Verify the rendered Android text still inserts soft wrap opportunities.
4. Only then revisit `EnrichedMarkdownTextLayoutManager.kt` and `MeasurementStore`.
5. Confirm on-device with the exact offending message instead of assuming a synthetic repro is equivalent.

## Files That Matter

- `xmtp-mobile/src/components/MessageBubble.tsx`
- `xmtp-mobile/metro.config.js`
- `vendor/react-native-enriched-markdown/android/src/main/java/com/swmansion/enriched/markdown/spans/CodeSpan.kt`
- `vendor/react-native-enriched-markdown/android/src/main/java/com/swmansion/enriched/markdown/views/CodeBlockContainerView.kt`
- `vendor/react-native-enriched-markdown/android/src/main/java/com/swmansion/enriched/markdown/renderer/CodeRenderer.kt`
- `vendor/react-native-enriched-markdown/android/src/main/java/com/swmansion/enriched/markdown/renderer/LinkRenderer.kt`
- `vendor/react-native-enriched-markdown/android/src/main/java/com/swmansion/enriched/markdown/renderer/SpanStyleCache.kt`
- `.github/workflows/mobile.yml`

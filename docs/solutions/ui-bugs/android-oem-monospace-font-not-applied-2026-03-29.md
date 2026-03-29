---
title: Android OEM devices ignore Typeface.MONOSPACE for inline code
date: 2026-03-29
category: ui-bugs
module: xmtp-mobile
problem_type: ui_bug
component: frontend_stimulus
symptoms:
  - Inline code in markdown messages renders with same font as body text
  - No visual distinction between inline code and regular text despite background/color being applied
  - Only observed on OnePlus 13 (ColorOS 16); standard Android emulators unaffected
root_cause: config_error
resolution_type: code_fix
severity: medium
tags: [android, monospace, font, coloros, oneplus, react-native, enriched-markdown, typeface, oem]
---

# Android OEM devices ignore Typeface.MONOSPACE for inline code

## Problem

Inline code in markdown chat messages renders with the same proportional font as body text on OnePlus/ColorOS devices, making code visually indistinguishable from regular text (aside from background color).

## Symptoms

- `react-native-enriched-markdown` CodeSpan sets `paint.typeface` to `Typeface.MONOSPACE` but the rendered text is not monospaced
- Background color and text color from `code` style ARE applied (proving the span runs)
- The font simply doesn't look different — ColorOS remaps the system monospace font alias

## What Didn't Work

1. **Setting `fontFamily: "monospace"` in JS markdownStyle** — The library's `normalizeMarkdownStyle` passes this to native as a string. Android's `Typeface.create("monospace", ...)` resolves to the same OEM-remapped font.

2. **Removing `fontFamily` to let library use default** — Without `fontFamily`, the library falls through to `SpanStyleCache.getMonospaceTypeface()` which calls `Typeface.create(Typeface.MONOSPACE, style)`. Same result — the OEM font alias is still remapped.

3. **Patching `SpanStyleCache.getMonospaceTypeface()` to load asset font** — Added `initAssetFonts(context)` to load JetBrains Mono from `assets/fonts/`. Called from `EnrichedMarkdown.init{}`. The method compiled into the dex, but the font still wasn't applied. Root cause: the `SpanStyleCache` companion object's `typefaceCache` may have been populated before `initAssetFonts` ran, or the cache key path was different from what `CodeSpan` used.

## Solution

Bypass the shared `SpanStyleCache` entirely. Load the bundled font directly inside `CodeSpan` itself using a `companion object` with lazy initialization:

**1. Bundle JetBrains Mono (4 weights) in Android assets:**
```
android/app/src/main/assets/fonts/
  JetBrainsMono-Regular.ttf
  JetBrainsMono-Bold.ttf
  JetBrainsMono-Italic.ttf
  JetBrainsMono-BoldItalic.ttf
```

**2. Patch `CodeSpan.kt` to load from assets directly:**
```kotlin
class CodeSpan(
  private val styleCache: SpanStyleCache,
  private val blockStyle: BlockStyle,
  private val context: Context? = null,  // NEW: pass context from CodeRenderer
) : MetricAffectingSpan() {

  private fun applyMonospacedFont(paint: TextPaint) {
    paint.textSize = if (styleCache.codeFontSize > 0) styleCache.codeFontSize else blockStyle.fontSize
    val preservedStyle = (paint.typeface?.style ?: 0) and (Typeface.BOLD or Typeface.ITALIC)
    // Try bundled font first, fall back to library default
    val assetTypeface = context?.let { loadAssetMonospace(it, preservedStyle) }
    paint.typeface = assetTypeface
      ?: SpanStyleCache.getMonospaceTypeface(preservedStyle)
  }

  companion object {
    private var cachedRegular: Typeface? = null
    private var loadAttempted = false

    private fun loadAssetMonospace(context: Context, style: Int): Typeface? {
      if (!loadAttempted) {
        loadAttempted = true
        try {
          cachedRegular = Typeface.createFromAsset(context.assets, "fonts/JetBrainsMono-Regular.ttf")
          // ... load Bold, Italic, BoldItalic variants
        } catch (_: Exception) {}
      }
      return /* style-matched variant */ cachedRegular
    }
  }
}
```

**3. Patch `CodeRenderer.kt` to pass context:**
```kotlin
CodeSpan(factory.styleCache, blockStyle, factory.context)
```

**4. Use `patch-package` to persist the patch:**
```json
"postinstall": "npx patch-package"
```

**5. Remove `fontFamily` from JS `code` style** — let the native side handle it entirely.

## Why This Works

Chinese Android OEMs (OnePlus/ColorOS, Xiaomi/MIUI, etc.) customize system fonts including the `monospace` font family alias. Both `Typeface.MONOSPACE` and `Typeface.create("monospace", ...)` resolve through the system font config, which these OEMs modify.

`Typeface.createFromAsset()` loads the TTF file directly from the APK, bypassing the system font resolution entirely. This guarantees a true monospace font regardless of OEM customization.

The shared `SpanStyleCache` approach failed because the cache is a companion object shared across all instances, and initialization timing relative to first cache access was unreliable. Loading directly in `CodeSpan` ensures the font is available exactly when needed.

## Prevention

- **Always bundle critical fonts as assets** — never rely on system font aliases for monospace on Android. OEM font customization is widespread in Chinese Android devices.
- **Test on real OEM devices** — emulators use stock Android fonts and won't surface this class of bug.
- **When patching native libraries**, verify the patched code is compiled into the dex by searching for unique strings: `strings classes*.dex | grep "YourString"`.
- **Prefer direct asset loading over cache-based initialization** for critical rendering paths — companion object caches have initialization ordering risks.

## Related Issues

- [react-native-enriched-markdown PR #104](https://github.com/software-mansion-labs/react-native-enriched-markdown/pull/104) — Added `fontFamily` support for inline code
- [react-native-enriched-markdown PR #128](https://github.com/software-mansion-labs/react-native-enriched-markdown/pull/128) — Similar class of bug where font style directives were ignored without custom fontFamily
- [facebook/react-native#20398](https://github.com/facebook/react-native/issues/20398) — Custom fonts not applied on nested text (Android)

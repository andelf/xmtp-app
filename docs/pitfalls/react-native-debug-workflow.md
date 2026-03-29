# React Native (Expo) Debug & Build Workflow

## Build Variants

### Debug APK (`assembleDebug`)

```bash
cd xmtp-mobile/android && ./gradlew assembleDebug
adb install -r app/build/outputs/apk/debug/app-debug.apk
```

- **JS bundle**: NOT embedded. App loads JS from Metro bundler at runtime.
- **Requires**: Metro running + `adb reverse tcp:8081 tcp:8081`.
- **If Metro is not running or port not forwarded**: App hangs on splash screen with no error visible to user.
- **console.log**: Visible in `adb logcat -s "ReactNativeJS:*"` (because Metro serves dev bundle).
- **Hot reload**: Supported.

### Release APK (`assembleRelease`)

```bash
cd xmtp-mobile/android && ./gradlew assembleRelease
adb install -r app/build/outputs/apk/release/app-release.apk
```

- **JS bundle**: Must be embedded manually (see below).
- **Does NOT need Metro** — runs standalone.
- **console.log**: Stripped if built with production bundle (`.hbc`). Preserved if built with `--dev` bundle (`.js`).

### Embedding JS Bundle for Release

**Production** (no logs, optimized):
```bash
npx expo export --platform android
mkdir -p android/app/src/main/assets
cp dist/_expo/static/js/android/*.hbc android/app/src/main/assets/index.android.bundle
```

**Dev bundle in release APK** (preserves console.log):
```bash
npx expo export --platform android --dev
mkdir -p android/app/src/main/assets
cp dist/_expo/static/js/android/*.js android/app/src/main/assets/index.android.bundle
```

Key difference: `--dev` generates `.js` (readable, logs preserved). Without `--dev`, generates `.hbc` (Hermes bytecode, **all console.log stripped**).

## Metro Bundler

```bash
# Start Metro (must be in xmtp-mobile/ directory)
npx expo start --dev-client

# If port conflict, Metro will ask to use another port — but the app
# hardcodes port 8081, so kill the conflicting process instead.

# USB port forward (required for physical devices)
adb reverse tcp:8081 tcp:8081
```

**Common failure**: Metro running but app stuck on splash screen.
- Check: `curl http://localhost:8081/status` → should return `packager-status:running`
- Check: `adb reverse tcp:8081 tcp:8081` → should print `8081`
- Check logcat for Metro errors: `adb logcat -d | grep -i metro`

## Logging

### Where to see logs

| Scenario | console.log visible? | How to read |
|----------|---------------------|-------------|
| Debug APK + Metro | Yes | `adb logcat -s "ReactNativeJS:*"` |
| Release APK + `--dev` bundle | Yes | `adb logcat -s "ReactNativeJS:*"` |
| Release APK + production bundle (`.hbc`) | **No** | Logs are compiled out |
| Native crash / no JS loaded | No JS logs | `adb logcat -d \| grep -i "ReactNative\|error\|fatal"` |

### Useful logcat commands

```bash
# Clear log buffer (do this before reproducing an issue)
adb logcat -c

# Stream JS logs in real-time
adb logcat -s "ReactNativeJS:*"

# Dump recent JS logs
adb logcat -d -s "ReactNativeJS:*" | tail -50

# Search for errors across all tags
adb logcat -d | grep -i -E "error|fatal|exception|ReactNative" | tail -30

# Search for specific keywords
adb logcat -d -s "ReactNativeJS:*" | grep -i "content\|unsupported"
```

### No JS logs at all?

Diagnosis checklist:
1. Is Metro running? (`curl http://localhost:8081/status`)
2. Is port forwarded? (`adb reverse tcp:8081 tcp:8081`)
3. Did the bundle load? Look for `Running "main"` in logcat
4. Check for Metro bundling errors: `adb logcat -d | grep -i metro`
5. Missing dependency? Metro will show `ENOENT` errors in logcat (e.g., `react-dom/client.js` not found after adding `@testing-library/react-native`)

## App Lifecycle Commands

```bash
# Force stop and restart
adb shell am force-stop com.anonymous.xmtpmobile
adb shell am start -n com.anonymous.xmtpmobile/.MainActivity

# Install and restart in one line
adb install -r <apk-path> && adb shell am force-stop com.anonymous.xmtpmobile && adb shell am start -n com.anonymous.xmtpmobile/.MainActivity
```

## Testing (Jest)

```bash
cd xmtp-mobile
npm test           # Run all tests
npx jest --no-cache  # Force fresh run
```

- Uses `babel-jest` with `@babel/preset-env` + `@babel/preset-typescript`.
- Does **NOT** use `jest-expo` preset (too heavy, pulls in Expo runtime).
- Pure function tests in `src/__tests__/` — import from `src/utils/` to avoid native SDK dependencies.
- For testing store logic, extract pure functions into `src/utils/` and test those directly.

## Gotchas We've Hit

### 1. Debug APK stuck on splash screen
**Cause**: Metro not running, or port not forwarded, or Metro has a bundling error.
**Fix**: Check Metro status, port forward, and logcat for errors.

### 2. `npm install` breaks Metro
**Cause**: Adding test dependencies (e.g., `@testing-library/react-native`) can pull in `react-dom` as a dependency. If `react-dom` is missing, Metro fails with `ENOENT: react-dom/client.js`.
**Fix**: `npm install react-dom --legacy-peer-deps`, then restart Metro.

### 3. console.log not appearing
**Cause**: Using production bundle (`.hbc`) which strips all console.log.
**Fix**: Either use debug APK + Metro, or build with `npx expo export --platform android --dev`.

### 4. `--legacy-peer-deps` needed for npm install
**Cause**: React Native ecosystem has peer dependency conflicts (React 19 + older libraries).
**Fix**: Always use `npm install --legacy-peer-deps` when adding new packages.

### 5. `debuggable true` in release builds
**Current state**: `android/app/build.gradle` has `debuggable true` on release builds for `adb shell run-as` access.
**Warning**: Must remove before any production release.

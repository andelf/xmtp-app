/**
 * Simple logger that writes to a global array and can be dumped.
 * For file logging, writes via native module (RNFS-free approach).
 *
 * Retrieve: adb shell run-as com.anonymous.xmtpmobile cat files/xmtp-app.log
 */

// In-memory log buffer (survives as long as JS runtime lives)
const MAX_ENTRIES = 500;
const entries: string[] = [];

function ts(): string {
  return new Date().toISOString().replace("T", " ").slice(0, 23);
}

export function log(tag: string, ...args: any[]) {
  const msg = args.map((a) => (typeof a === "string" ? a : JSON.stringify(a))).join(" ");
  const line = `${ts()} [${tag}] ${msg}`;
  entries.push(line);
  if (entries.length > MAX_ENTRIES) entries.shift();

  // Also native log (visible in adb logcat -s ReactNativeJS even in release if debuggable=true)
  console.log(`[${tag}]`, ...args);
}

/** Get all buffered log entries as a single string. */
export function getLogs(): string {
  return entries.join("\n");
}

/** Clear the buffer. */
export function clearLogs() {
  entries.length = 0;
}

/** Get entry count. */
export function logCount(): number {
  return entries.length;
}

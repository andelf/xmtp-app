/**
 * Time formatting utilities.
 */

/**
 * Format a millisecond-epoch timestamp into a human-friendly relative string.
 *
 * Rules:
 *  - < 1 minute  -> "just now"
 *  - < 1 hour    -> "Xm ago"
 *  - < 24 hours  -> "Xh ago"
 *  - yesterday    -> "yesterday"
 *  - otherwise    -> "MM/DD"
 */
export function formatRelativeTime(timestampMs: number): string {
  const now = Date.now();
  const diffMs = now - timestampMs;

  if (diffMs < 0) {
    return "just now";
  }

  const ONE_MINUTE = 60 * 1000;
  const ONE_HOUR = 60 * ONE_MINUTE;
  const ONE_DAY = 24 * ONE_HOUR;

  if (diffMs < ONE_MINUTE) {
    return "just now";
  }

  if (diffMs < ONE_HOUR) {
    const mins = Math.floor(diffMs / ONE_MINUTE);
    return `${mins}m ago`;
  }

  if (diffMs < ONE_DAY) {
    const hours = Math.floor(diffMs / ONE_HOUR);
    return `${hours}h ago`;
  }

  // Check if it was yesterday
  const todayStart = new Date();
  todayStart.setHours(0, 0, 0, 0);
  const yesterdayStart = new Date(todayStart.getTime() - ONE_DAY);

  const msgDate = new Date(timestampMs);
  if (msgDate >= yesterdayStart && msgDate < todayStart) {
    return "yesterday";
  }

  // Fallback: MM/DD
  const month = String(msgDate.getMonth() + 1).padStart(2, "0");
  const day = String(msgDate.getDate()).padStart(2, "0");
  return `${month}/${day}`;
}

/**
 * Format a millisecond-epoch timestamp into HH:mm (24-hour) for message bubbles.
 */
export function formatMessageTime(timestampMs: number): string {
  const d = new Date(timestampMs);
  const h = String(d.getHours()).padStart(2, "0");
  const m = String(d.getMinutes()).padStart(2, "0");
  return `${h}:${m}`;
}

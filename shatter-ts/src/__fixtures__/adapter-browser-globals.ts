/**
 * Adapter fixture: browser globals (window, document, localStorage).
 *
 * These functions reference browser APIs that are unavailable in Node.js.
 * A browser-globals adapter must polyfill or stub these before execution.
 *
 * getViewportWidth: reads window.innerWidth.
 *   - window.innerWidth > 768  → "desktop"
 *   - window.innerWidth <= 768 → "mobile"
 *
 * getTitle: reads document.title.
 *   - document.title is truthy → returns it
 *   - document.title is falsy  → returns "Untitled"
 *
 * readSetting: reads localStorage.getItem().
 *   - stored value exists   → returns parsed value
 *   - stored value is null  → returns fallback
 *
 * setBodyClass: writes document.body.className.
 *   - className is truthy → sets it
 *   - className is falsy  → clears to ""
 */

export function getViewportWidth(): string {
  const width = window.innerWidth;
  if (width > 768) {
    return "desktop";
  }
  return "mobile";
}

export function getTitle(): string {
  const title = document.title;
  if (title) {
    return title;
  }
  return "Untitled";
}

export function readSetting(key: string, fallback: string): string {
  const stored = localStorage.getItem(key);
  if (stored !== null) {
    return stored;
  }
  return fallback;
}

export function setBodyClass(className: string): string {
  if (className) {
    document.body.className = className;
  } else {
    document.body.className = "";
  }
  return document.body.className;
}

/**
 * The desktop system the app runs on. The web view's user agent names it
 * (WebView2 on Windows, WebKitGTK on Linux), so nothing has to be asked of
 * the backend before the first frame.
 */
export function isLinuxUserAgent(userAgent: string): boolean {
  return /Linux/i.test(userAgent) && !/Android/i.test(userAgent);
}

export const IS_LINUX = typeof navigator !== "undefined" && isLinuxUserAgent(navigator.userAgent);

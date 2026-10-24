(() => {
  const IMAGE_EXTENSIONS = /\.(?:avif|bmp|gif|jpe?g|png|svg|webp)$/i;

  function isImageUrl(value) {
    if (typeof value !== "string") return false;
    try {
      return IMAGE_EXTENSIONS.test(new URL(value).pathname);
    } catch {
      return false;
    }
  }

  function imageDownloadTarget(info) {
    if (typeof info?.srcUrl === "string" && info.srcUrl) return info.srcUrl;
    if (info?.mediaType === "image" && typeof info.linkUrl === "string" && info.linkUrl) {
      return info.linkUrl;
    }
    return isImageUrl(info?.linkUrl) ? info.linkUrl : null;
  }

  function videoDownloadTarget(info) {
    try {
      const page = new URL(info.pageUrl);
      const site = page.hostname.toLowerCase();
      if (/^https?:$/.test(page.protocol) && ["youtube.com", "youtu.be", "youtube-nocookie.com", "vimeo.com", "aparat.com", "dailymotion.com", "twitch.tv", "pornhub.com", "pinterest.com"].some(host => site === host || site.endsWith(`.${host}`))) {
        return { type: "inspect", url: page.href };
      }
    } catch { /* fall back to an actual media source */ }
    try {
      const source = new URL(info.srcUrl);
      if (/^https?:$/.test(source.protocol)) return { type: "download", url: source.href };
    } catch { /* blob/data URLs cannot be downloaded outside the browser */ }
    return null;
  }

  globalThis.RatatoskrContextRouting = { imageDownloadTarget, videoDownloadTarget };
})();

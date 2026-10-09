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

  globalThis.RatatoskrContextRouting = { imageDownloadTarget };
})();

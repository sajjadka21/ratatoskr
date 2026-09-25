// Fills every [data-i18n] element from the extension's messages and sets the
// page direction for right-to-left languages.
for (const element of document.querySelectorAll("[data-i18n]")) {
  element.textContent = chrome.i18n.getMessage(element.dataset.i18n);
}
const language = chrome.i18n.getUILanguage();
document.documentElement.lang = language;
document.documentElement.dir = /^(fa|ar|he|ur)\b/i.test(language) ? "rtl" : "ltr";

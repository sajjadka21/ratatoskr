// Run before the stylesheet so a saved preference is applied before first paint.
(() => {
  document.documentElement.classList.add('js');
  let saved;
  try { saved = localStorage.getItem('ratatoskr-appearance'); } catch {}
  const mode = saved === 'dark' || saved === 'light' ? saved : (matchMedia('(prefers-color-scheme: dark)').matches ? 'dark' : 'light');
  document.documentElement.dataset.appearance = mode;
})();

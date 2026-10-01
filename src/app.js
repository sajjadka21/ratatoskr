const isFa = document.documentElement.lang === 'fa';
const reducedMotion = matchMedia('(prefers-reduced-motion: reduce)');
const header = document.querySelector('.site-header');
const menuToggle = document.querySelector('.menu-toggle');
function setMenu(open,restoreFocus=false) {
  header.dataset.menuOpen=String(open);
  menuToggle.setAttribute('aria-expanded',String(open));
  menuToggle.setAttribute('aria-label',open?(isFa?'بستن منو':'Close navigation'):(isFa?'باز کردن منو':'Open navigation'));
  if(restoreFocus)menuToggle.focus();
}
menuToggle.addEventListener('click',()=>setMenu(header.dataset.menuOpen!=='true'));
header.querySelectorAll('a').forEach(link=>link.addEventListener('click',()=>{
  const fromOpenMenu=header.dataset.menuOpen==='true';
  setMenu(false);
  if(fromOpenMenu&&link.hash){const target=document.querySelector(link.hash);if(target){target.setAttribute('tabindex','-1');target.focus({preventScroll:true});}}
}));
document.addEventListener('keydown',event=>{if(event.key==='Escape'&&header.dataset.menuOpen==='true')setMenu(false,true)});
document.addEventListener('click',event=>{if(!header.contains(event.target))setMenu(false)});
const mobileQuery=matchMedia('(max-width: 900px)');
mobileQuery.addEventListener('change',()=>setMenu(false));
const sections=[...document.querySelectorAll('.hero,#about,#themes,#faq,#download')];
const navObserver=new IntersectionObserver(entries=>{
  const visible=entries.filter(entry=>entry.isIntersecting).sort((a,b)=>b.intersectionRatio-a.intersectionRatio)[0];
  if(!visible)return;
  header.querySelectorAll('a[href^="#"]').forEach(link=>{
    if(link.hash==='#'+visible.target.id)link.setAttribute('aria-current','location');else link.removeAttribute('aria-current');
  });
},{rootMargin:'-15% 0px -45% 0px',threshold:[0,.2,.5]});
sections.forEach(section=>navObserver.observe(section));
function animatePreview(image) {
  if(reducedMotion.matches)return;
  image.getAnimations().forEach(animation=>animation.cancel());
  image.animate([{opacity:.65,transform:'translateY(5px) scale(.992)'},{opacity:1,transform:'none'}],{duration:320,easing:'cubic-bezier(.2,0,0,1)'});
}
const appearanceToggle = document.querySelector('.appearance-toggle');
const appearanceQuery = matchMedia('(prefers-color-scheme: dark)');
let appearanceChosen = false;
try { appearanceChosen = ['dark','light'].includes(localStorage.getItem('ratatoskr-appearance')); } catch {}
function setAppearance(mode) {
  document.documentElement.dataset.appearance = mode;
  const label = mode === 'dark' ? (isFa ? 'تغییر به حالت روشن' : 'Switch to light mode') : (isFa ? 'تغییر به حالت تاریک' : 'Switch to dark mode');
  appearanceToggle.setAttribute('aria-label',label);
  appearanceToggle.title = label;
  document.querySelector('meta[name="theme-color"]').content = mode === 'dark' ? '#10131c' : '#e7e1d5';
}
setAppearance(document.documentElement.dataset.appearance || 'light');
appearanceToggle.addEventListener('click', () => {
  const mode = document.documentElement.dataset.appearance === 'dark' ? 'light' : 'dark';
  appearanceChosen = true;
  setAppearance(mode);
  try { localStorage.setItem('ratatoskr-appearance',mode); } catch {}
});
appearanceQuery.addEventListener('change',event => { if (!appearanceChosen) setAppearance(event.matches ? 'dark' : 'light'); });
const themes = {
  'midnight-arcane': {name:'Midnight Arcane',description:isFa?'نور آمتیست، شاخه‌های درهم‌تنیده و آرامش نیمه‌شب.':'Amethyst light, intricate branches, and the stillness of midnight.',palette:["#090B14","#141225","#CAA0FD","#9F63FF","#F2EBFE"]},
  'ember-forge': {name:'Ember Forge',description:isFa?'گرمای کهربا و نقش‌هایی که در روشنای اخگر می‌درخشند.':'Warm amber, copper branches, and the quiet glow of embers.',palette:["#110C0A","#211610","#FBA73D","#F46803","#FFF1DB"]},
  'forest-rune': {name:'Forest Rune',description:isFa?'سبز خزه‌ای، برگ‌های بلوط و نقش‌های رونیک جنگل.':'Moss greens, oak leaves, and runes rooted in the forest.',palette:["#0A100B","#152016","#BFCE8B","#8CB34B","#F0F3E5"]},
  'frost-byte': {name:'Frost Byte',description:isFa?'آبی یخی، جزئیات نقره‌ای و روشنای شفاف زمستان.':'Ice blue, silver details, and the clarity of a winter morning.',palette:["#071019","#0E1D2B","#98D9FB","#53B4F4","#DEF4FD"]}
};
const tabs = [...document.querySelectorAll('[role="tab"]')];
function selectTheme(slug,focus=false){
  if (!Object.hasOwn(themes,slug)) return;
  const theme=themes[slug];
  document.querySelector('.hero-art').dataset.theme=slug;
  document.querySelector('#hero-theme-name').textContent=theme.name;
  const heroPreview=document.querySelector('#hero-preview');
  const nextPreview='/assets/screen-'+slug+'.jpg';
  if(heroPreview.getAttribute('src')!==nextPreview){heroPreview.src=nextPreview;heroPreview.decode().then(()=>{if(heroPreview.getAttribute('src')===nextPreview)animatePreview(heroPreview)}).catch(()=>{});}
  heroPreview.alt=isFa?'تصویر رابط واقعی راتاتوسک در تم '+theme.name:theme.name+' actual interface, captured in a browser';
  document.querySelectorAll('[data-preview-theme]').forEach(button=>button.setAttribute('aria-pressed',String(button.dataset.previewTheme===slug)));
  document.querySelector('.themes-section').dataset.theme=slug;
  for(const tab of tabs){const active=tab.dataset.theme===slug;tab.setAttribute('aria-selected',String(active));tab.tabIndex=active?0:-1;if(active&&focus)tab.focus();}
  document.querySelector('#theme-panel').setAttribute('aria-labelledby','tab-'+slug);
  document.querySelector('#theme-name').textContent=theme.name;
  document.querySelector('#theme-detail-name').textContent=theme.name;
  document.querySelector('#theme-description').textContent=theme.description;
  const preview=document.querySelector('#theme-preview');
  if(preview.getAttribute('src')!==nextPreview){preview.src=nextPreview;preview.decode().then(()=>{if(preview.getAttribute('src')===nextPreview)animatePreview(preview)}).catch(()=>{});}
  preview.alt=isFa?'تصویر رابط واقعی راتاتوسک در تم '+theme.name:theme.name+' actual interface, captured in a browser';
  document.querySelector('#theme-icon').src='/assets/icon-'+slug+'.webp';
  const palette=document.querySelector('#theme-palette');palette.replaceChildren();palette.setAttribute('aria-label',isFa?'پالت رنگ '+theme.name:theme.name+' color palette');
  for(const color of theme.palette){const span=document.createElement('span');span.style.backgroundColor=color;span.title=color;span.setAttribute('aria-label',color);palette.append(span);}
  try{localStorage.setItem('ratatoskr-theme',slug);}catch{/* Theme selection still works when storage is blocked. */}
}
for(const tab of tabs){
  tab.addEventListener('click',()=>selectTheme(tab.dataset.theme));
  tab.addEventListener('keydown',event=>{
    let index=tabs.indexOf(tab);
    if(event.key==='ArrowRight')index=(index+(isFa?-1:1)+tabs.length)%tabs.length;
    else if(event.key==='ArrowLeft')index=(index+(isFa?1:-1)+tabs.length)%tabs.length;
    else if(event.key==='Home')index=0;
    else if(event.key==='End')index=tabs.length-1;
    else return;
    event.preventDefault();selectTheme(tabs[index].dataset.theme,true);
  });
}
let storedTheme='ember-forge';try{storedTheme=localStorage.getItem('ratatoskr-theme')||storedTheme;}catch{}
selectTheme(Object.hasOwn(themes,storedTheme)?storedTheme:'ember-forge');
document.querySelectorAll('[data-preview-theme]').forEach(button=>button.addEventListener('click',()=>selectTheme(button.dataset.previewTheme)));
document.querySelector('#year').textContent=new Date().getFullYear();


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
  document.querySelector('meta[name="theme-color"]').content = mode === 'dark' ? '#10131c' : '#f9fafb';
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
  'midnight-arcane': {name:'Midnight Arcane',description:isFa?'نور آمتیست، شاخه‌های درهم‌تنیده و آرامش نیمه‌شب.':'Amethyst light, intricate branches, and the stillness of midnight.',palette:['#1A1047','#461BA3','#7F34FC','#CAA0FD','#F2EBFE']},
  'ember-forge': {name:'Ember Forge',description:isFa?'گرمای کهربا و نقش‌هایی که در روشنای اخگر می‌درخشند.':'Warm amber, copper branches, and the quiet glow of embers.',palette:['#43170D','#922117','#F46803','#FBA73D','#FEDD9E']},
  'forest-rune': {name:'Forest Rune',description:isFa?'سبز خزه‌ای، برگ‌های بلوط و نقش‌های رونیک جنگل.':'Moss greens, oak leaves, and runes rooted in the forest.',palette:['#263822','#517530','#8CB34B','#BFCE8B','#F0E7BF']},
  'frost-byte': {name:'Frost Byte',description:isFa?'آبی یخی، جزئیات نقره‌ای و روشنای شفاف زمستان.':'Ice blue, silver details, and the clarity of a winter morning.',palette:['#063168','#1167CD','#53B4F4','#98D9FB','#DEF4FD']}
};
const tabs = [...document.querySelectorAll('[role="tab"]')];
function selectTheme(slug,focus=false){
  if (!Object.hasOwn(themes,slug)) return;
  const theme=themes[slug];
  document.querySelector('.hero-art').dataset.theme=slug;
  document.querySelector('#hero-theme-name').textContent=theme.name;
  const heroPreview=document.querySelector('#hero-preview');
  const nextPreview='/assets/ui-'+slug+'.webp';
  if(heroPreview.getAttribute('src')!==nextPreview){heroPreview.src=nextPreview;heroPreview.decode().then(()=>{if(heroPreview.getAttribute('src')===nextPreview)animatePreview(heroPreview)}).catch(()=>{});}
  heroPreview.alt=isFa?'پیش‌نمایش طراحی رابط '+theme.name:theme.name+' interface design preview';
  document.querySelectorAll('[data-preview-theme]').forEach(button=>button.setAttribute('aria-pressed',String(button.dataset.previewTheme===slug)));
  document.querySelector('.themes-section').dataset.theme=slug;
  for(const tab of tabs){const active=tab.dataset.theme===slug;tab.setAttribute('aria-selected',String(active));tab.tabIndex=active?0:-1;if(active&&focus)tab.focus();}
  document.querySelector('#theme-panel').setAttribute('aria-labelledby','tab-'+slug);
  document.querySelector('#theme-name').textContent=theme.name;
  document.querySelector('#theme-detail-name').textContent=theme.name;
  document.querySelector('#theme-description').textContent=theme.description;
  const preview=document.querySelector('#theme-preview');
  if(preview.getAttribute('src')!==nextPreview){preview.src=nextPreview;preview.decode().then(()=>{if(preview.getAttribute('src')===nextPreview)animatePreview(preview)}).catch(()=>{});}
  preview.alt=isFa?'پیش‌نمایش طراحی رابط '+theme.name:theme.name+' interface design preview';
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
let storedTheme='midnight-arcane';try{storedTheme=localStorage.getItem('ratatoskr-theme')||storedTheme;}catch{}
selectTheme(Object.hasOwn(themes,storedTheme)?storedTheme:'midnight-arcane');
document.querySelectorAll('[data-preview-theme]').forEach(button=>button.addEventListener('click',()=>selectTheme(button.dataset.previewTheme)));
document.querySelector('#year').textContent=new Date().getFullYear();

const words=isFa?{ready:'نسخهٔ انتشار آماده است',download:'دریافت',date:'تاریخ انتشار',version:'نسخه',unavailable:'هنوز فایل دانلودی برای این سیستم منتشر نشده است.',pending:'در انتظار انتشار',preview:'نسخهٔ پیش‌انتشار',summary:'فایل مناسب سیستم خود را انتخاب کنید. جزئیات هر نسخه را در یادداشت‌های انتشار بخوانید.'}:{ready:'A release is ready',download:'Download',date:'Released',version:'Version',unavailable:'No download file has been published for this platform yet.',pending:'Awaiting a build',preview:'Pre-release',summary:'Choose the file for your system. Read the release notes for the details of this build.'};
function safeUrl(value){try{const url=new URL(value);return url.protocol==='https:'?url.href:null;}catch{return null;}}
function configureLink(id,url){const target=document.getElementById(id);const safe=safeUrl(url);if(!safe)return;target.href=safe;target.hidden=false;target.rel='noopener noreferrer';}
function applyRelease(config){
  if(typeof config.repository==='string'&&/^[A-Za-z0-9_.-]+\/[A-Za-z0-9_.-]+$/.test(config.repository))configureLink('project-link','https://github.com/'+config.repository);
  const release=config.release;if(!release||!Array.isArray(release.downloads))return;
  configureLink('release-notes',release.notesUrl);
  const downloads=release.downloads.filter(item=>['windows','macos','linux'].includes(item.platform)&&safeUrl(item.url));
  if(!downloads.length)return;
  document.querySelector('#hero-status').textContent=words.ready;
  document.querySelector('#hero-status').parentElement.setAttribute('aria-label',words.ready);
  const status=document.querySelector('#release-state');status.replaceChildren();status.append(document.createElement('span'),document.createTextNode(words.ready+(release.version?' · '+release.version:'')));
  document.querySelector('#release-summary').textContent=words.summary;
  const panels=document.querySelectorAll('#release-downloads article');
  for(const [i,platform] of ['windows','macos','linux'].entries()){
    const panel=panels[i],available=downloads.filter(item=>item.platform===platform);
    const description=panel.querySelector('p');description.textContent=available.length?(release.version?words.version+' '+release.version:words.ready):words.unavailable;
    const status=panel.querySelector('.availability');status.remove();
    if(!available.length){const pending=document.createElement('span');pending.className='availability';pending.textContent=words.pending;panel.append(pending);continue;}
    for(const item of available){const link=document.createElement('a');link.className='button button-gold';link.href=safeUrl(item.url);link.rel='noopener noreferrer';link.textContent=item.label||words.download+' '+({windows:'Windows',macos:'macOS',linux:'Linux'}[platform]);panel.append(link);}
  }
  const date=new Date(release.publishedAt);
  document.querySelector('#release-date').textContent=release.publishedAt&&!Number.isNaN(date.getTime())?words.date+' '+new Intl.DateTimeFormat(isFa?'fa-IR':'en',{dateStyle:'medium',timeZone:'UTC'}).format(date):(release.version?words.version+' '+release.version:words.ready);
  document.querySelector('.hero-note').textContent=isFa?'برای امکانات قطعی و سازگاری سیستم، یادداشت‌های انتشار را بخوانید.':'Read the release notes for confirmed features and system compatibility.';
}
fetch('/site-config.json',{cache:'no-cache'}).then(response=>{if(!response.ok)throw new Error('Release configuration unavailable');return response.json();}).then(applyRelease).catch(()=>{/* Keep the truthful server-rendered development state available. */});

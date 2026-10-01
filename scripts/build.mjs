import fs from 'node:fs/promises';
import path from 'node:path';
import {fileURLToPath} from 'node:url';
import fa from './fa.mjs';
import {releaseContent} from './release-content.mjs';
const root=path.resolve(path.dirname(fileURLToPath(import.meta.url)),'..');
const src=path.join(root,'src'),dist=path.join(root,'dist');
const config=JSON.parse(await fs.readFile(path.join(root,'site.config.json'),'utf8'));
const httpsUrl=value=>{const url=new URL(value);if(url.protocol!=='https:')throw new Error('Public links must use HTTPS');return url.href;};
if(config.siteUrl)config.siteUrl=httpsUrl(config.siteUrl).replace(/\/$/,'');
if(config.repository&&!/^[A-Za-z0-9_.-]+\/[A-Za-z0-9_.-]+$/.test(config.repository))throw new Error('Repository must be owner/repository');
if(config.release.notesUrl)httpsUrl(config.release.notesUrl);
if(config.release.checksumsUrl)httpsUrl(config.release.checksumsUrl);
if(config.release.downloads.length && (!config.release.version || !Number.isFinite(Date.parse(config.release.publishedAt))))throw new Error("Published release needs a version and date");
for(const item of config.release.downloads){if(!['windows','android'].includes(item.platform))throw new Error('Unknown platform');httpsUrl(item.url);}
await fs.mkdir(dist,{recursive:true});await fs.cp(src,dist,{recursive:true});
let en=await fs.readFile(path.join(src,'index.html'),'utf8');
const escape=s=>s.replace(/[&<>"']/g,c=>({'&':'&amp;','<':'&lt;','>':'&gt;','"':'&quot;',"'":'&#39;'}[c]));
function metadata(lang){
 if(!config.siteUrl)return '';
 const origin=escape(config.siteUrl),url=origin+(lang==='fa'?'/fa/':'/');
 return `<link rel="canonical" href="${url}"><link rel="alternate" hreflang="en" href="${origin}/"><link rel="alternate" hreflang="fa" href="${origin}/fa/"><link rel="alternate" hreflang="x-default" href="${origin}/"><meta property="og:url" content="${url}"><meta property="og:image" content="${origin}/assets/social-preview.jpg"><meta property="og:locale" content="${lang==='fa'?'fa_IR':'en_US'}">`;
}
let persian=en.replace('<html lang="en">','<html lang="fa" dir="rtl">').replace('<a class="language-switch" href="/fa/" lang="fa" hreflang="fa">فارسی</a>','<a class="language-switch" href="/" lang="en" hreflang="en">English</a>');
for(const [original,translation] of Object.entries(fa).sort((a,b)=>b[0].length-a[0].length))persian=persian.replaceAll(original,translation);
await fs.writeFile(path.join(dist,'index.html'),releaseContent(en,config,'en',escape).replace('<!-- SITE_META -->',metadata('en')));
await fs.mkdir(path.join(dist,'fa'),{recursive:true});await fs.writeFile(path.join(dist,'fa','index.html'),releaseContent(persian,config,'fa',escape).replace('<!-- SITE_META -->',metadata('fa')));
await fs.writeFile(path.join(dist,'site-config.json'),JSON.stringify(config,null,2));
await fs.writeFile(path.join(dist,'robots.txt'),'User-agent: *\nAllow: /\n'+(config.siteUrl?'Sitemap: '+config.siteUrl+'/sitemap.xml\n':''));
if(config.siteUrl){const o=escape(config.siteUrl);await fs.writeFile(path.join(dist,'sitemap.xml'),`<?xml version="1.0" encoding="UTF-8"?><urlset xmlns="http://www.sitemaps.org/schemas/sitemap/0.9"><url><loc>${o}/</loc></url><url><loc>${o}/fa/</loc></url></urlset>`);}
await fs.writeFile(path.join(dist,'404.html'),'<!doctype html><html lang="en"><meta charset="utf-8"><meta name="viewport" content="width=device-width,initial-scale=1"><title>Page not found — Ratatoskr</title><link rel="stylesheet" href="/styles.css"><main class="wrap section"><h1>That path leads elsewhere.</h1><p>The page you’re looking for isn’t here.</p><a class="button button-gold" href="/">Return to Ratatoskr</a><p lang="fa" dir="rtl">این صفحه پیدا نشد. <a href="/fa/">بازگشت به صفحهٔ فارسی</a></p></main></html>');
console.log('Built English + Persian static pages for '+(config.siteUrl||'a preview domain'));

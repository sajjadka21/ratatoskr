import fs from 'node:fs/promises';
const configPath=new URL('../site.config.json',import.meta.url);
const config=JSON.parse(await fs.readFile(configPath,'utf8'));
if(!/^[A-Za-z0-9_.-]+\/[A-Za-z0-9_.-]+$/.test(config.repository))throw new Error('Set the real public owner/repository first');
const response=await fetch('https://api.github.com/repos/'+config.repository+'/releases/latest',{headers:{Accept:'application/vnd.github+json','User-Agent':'Ratatoskr-landing'}});
if(!response.ok)throw new Error('GitHub release lookup failed: '+response.status);
const release=await response.json();
if(release.draft||release.prerelease||!/^v\d+\.\d+\.\d+$/.test(release.tag_name))throw new Error('A public stable version tag is required');
const version=release.tag_name.slice(1),assets=new Map((release.assets||[]).map(a=>[a.name,a]));
const asset=name=>{const value=assets.get(name);const expected='https://github.com/'+config.repository+'/releases/download/'+release.tag_name+'/'+name;if(!value||value.state!=='uploaded'||value.size<1||value.browser_download_url!==expected||!/^sha256:[a-f0-9]{64}$/.test(value.digest||''))throw new Error('Missing or invalid published asset: '+name);return value;};
const checksums=asset('SHA256SUMS.txt');
const checksumResponse=await fetch(checksums.browser_download_url);if(!checksumResponse.ok)throw new Error('Checksum download failed');
const sums=new Map((await checksumResponse.text()).trim().split(/\r?\n/).map(line=>line.split('  ').reverse()));
const mappings=[
 [`Ratatoskr_${version}_x64-setup.exe`,'windows','Download for Windows','دریافت نصب‌کنندهٔ ویندوز'],
 ['Ratatoskr-portable.zip','windows','Portable version','نسخهٔ قابل‌حمل'],
 [`Ratatoskr_${version}_x64_en-US.msi`,'windows','MSI installer','نصب‌کنندهٔ MSI'],
 ['Ratatoskr-android.apk','android','Download Android APK','دریافت نسخهٔ اندروید']
];
const expectedNames=[...mappings.map(x=>x[0]),`Ratatoskr_${version}_x64-setup.exe.sig`,'Ratatoskr-android-source.zip','latest.json'];
if(sums.size!==expectedNames.length)throw new Error('Incomplete release checksum manifest');
for(const name of expectedNames)if(asset(name).digest!=='sha256:'+sums.get(name))throw new Error('Published checksum mismatch: '+name);
config.stage='published-release';
config.release={version,publishedAt:release.published_at,notesUrl:release.html_url,checksumsUrl:checksums.browser_download_url,downloads:mappings.map(([name,platform,label,labelFa])=>{const a=asset(name);return{platform,label,labelFa,url:a.browser_download_url,bytes:a.size,sha256:a.digest.slice(7)}})};
await fs.writeFile(configPath,JSON.stringify(config,null,2)+'\n');
console.log('Connected verified stable release '+version+' with four download links and matching asset checksums.');

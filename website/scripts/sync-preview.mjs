import fs from 'node:fs/promises';
const version=process.argv[2];
if(!/^\d+\.\d+\.\d+$/.test(version||''))throw Error('Expected a version');
const configPath=new URL('../site.config.json',import.meta.url);
const config=JSON.parse(await fs.readFile(configPath,'utf8'));
if(!/^[\w.-]+\/[\w.-]+$/.test(config.repository))throw Error('Invalid repository');
const response=await fetch('https://api.github.com/repos/'+config.repository+'/releases/tags/v'+version,{headers:{Accept:'application/vnd.github+json','User-Agent':'Ratatoskr-landing'}});
if(!response.ok)throw Error('Published preview lookup failed: '+response.status);
const release=await response.json();
if(release.draft||!release.prerelease||release.tag_name!=='v'+version)throw Error('Expected a public preview');
const assets=new Map(release.assets.map(a=>[a.name,a]));
const expected=[`Ratatoskr_${version}_x64-setup.exe`,`Ratatoskr_${version}_x64-setup.exe.sig`,
 `Ratatoskr_${version}_x64_en-US.msi`,'Ratatoskr-portable.zip','Ratatoskr-android.apk','Ratatoskr-android-source.zip','latest.json',
 ...['arm64-v8a','armeabi-v7a','x86_64'].map(abi=>'Ratatoskr-android-'+abi+'.apk'),
 ...['chrome','edge','firefox'].map(browser=>'Ratatoskr-extension-'+browser+'.zip')];
function asset(name){
 const a=assets.get(name);
 if(!a||a.state!=='uploaded'||a.size<1||!/^sha256:[a-f0-9]{64}$/.test(a.digest||'')||
 a.browser_download_url!=='https://github.com/'+config.repository+'/releases/download/v'+version+'/'+name)throw Error('Invalid asset: '+name);
 return a;
}
const checksum=asset('SHA256SUMS.txt');
const sumsResponse=await fetch(checksum.browser_download_url);
if(!sumsResponse.ok)throw Error('Checksum lookup failed');
const sums=new Map((await sumsResponse.text()).trim().split(/\r?\n/).map(line=>line.split('  ').reverse()));
if(sums.size!==expected.length||assets.size!==expected.length+1)throw Error('Incomplete or unexpected preview assets');
for(const name of expected)if(asset(name).digest!=='sha256:'+sums.get(name))throw Error('Checksum mismatch: '+name);
config.preview={version,publishedAt:release.published_at,notesUrl:release.html_url,checksumsUrl:checksum.browser_download_url,
 windowsUrl:asset(`Ratatoskr_${version}_x64-setup.exe`).browser_download_url,
 androidUrl:asset('Ratatoskr-android.apk').browser_download_url,
 android32Url:asset('Ratatoskr-android-armeabi-v7a.apk').browser_download_url};
await fs.writeFile(configPath,JSON.stringify(config,null,2)+'\n');
console.log('Connected verified preview '+version+'; stable release retained');

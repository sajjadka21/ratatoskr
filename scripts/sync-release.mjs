import fs from 'node:fs/promises';
const configPath=new URL('../site.config.json',import.meta.url);
const config=JSON.parse(await fs.readFile(configPath,'utf8'));
if(!/^[A-Za-z0-9_.-]+\/[A-Za-z0-9_.-]+$/.test(config.repository)){console.error('Add your real public GitHub owner/repository in site.config.json first.');process.exit(1);}
const response=await fetch('https://api.github.com/repos/'+config.repository+'/releases/latest',{headers:{Accept:'application/vnd.github+json','User-Agent':'Ratatoskr-landing'}});
if(!response.ok)throw new Error('GitHub release lookup failed: '+response.status);
const data=await response.json();
if(data.draft||data.prerelease)throw new Error('Latest stable release required');
config.release.version=data.tag_name;config.release.publishedAt=data.published_at;config.release.notesUrl=data.html_url;
// Do not guess architecture or OS from ambiguous filenames. Explicit download mappings remain user-owned.
await fs.writeFile(configPath,JSON.stringify(config,null,2)+'\n');
console.log('Release metadata updated. Set explicit platform download URLs from these assets:');
for(const asset of data.assets||[])console.log(asset.name+'  '+asset.browser_download_url);

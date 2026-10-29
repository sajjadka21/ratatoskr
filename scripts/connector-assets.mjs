import fs from 'node:fs';
import path from 'node:path';
import {createRequire} from 'node:module';
const require=createRequire(import.meta.url);
const {hash}=require('blake3-wasm');
const root=path.resolve('dist');
const oldManifest=JSON.parse(fs.readFileSync('../../work/worker-manifest.json','utf8'));
const manifest={},files={};
function walk(dir){
 for(const entry of fs.readdirSync(dir,{withFileTypes:true})){
  const full=path.join(dir,entry.name);
  if(entry.isDirectory()){walk(full);continue;}
  const data=fs.readFileSync(full),value=data.toString('base64');
  const digest=hash(value+path.extname(full).slice(1)).toString('hex').slice(0,32);
  const url='/'+path.relative(root,full).replaceAll('\\','/');
  manifest[url]={hash:digest,size:data.length};
  if(oldManifest[url]?.hash!==digest)files[digest]={value};
 }
}
walk(root);
fs.writeFileSync('../../work/worker-release-1.3.0-payload.json',JSON.stringify({manifest,files}));
console.log(JSON.stringify({assets:Object.keys(manifest).length,changed:Object.keys(files).length}));

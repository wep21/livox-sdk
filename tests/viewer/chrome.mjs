// Node 22+; connect only to a dedicated Chrome debugging profile. No npm deps.
// Usage: node tests/viewer/chrome.mjs VIEWER_URL [CHROME_DEBUG_URL]
import assert from 'node:assert/strict';
import {writeFile} from 'node:fs/promises';
const url = process.argv[2] ?? 'http://127.0.0.1:8080';
const chrome = process.argv[3] ?? 'http://127.0.0.1:9222';
const target = await (await fetch(`${chrome}/json/new?about:blank`, {method:'PUT'})).json();
const ws = new WebSocket(target.webSocketDebuggerUrl);
await new Promise((resolve, reject) => {ws.addEventListener('open',resolve,{once:true});ws.addEventListener('error',reject,{once:true});});
let seq=0;
const pending=new Map(), errors=[];
ws.addEventListener('message', ({data}) => {
  const msg=JSON.parse(data);
  if (msg.id) {
    const p=pending.get(msg.id); if(!p)return;
    pending.delete(msg.id);clearTimeout(p.timer);
    msg.error?p.reject(msg.error):p.resolve(msg.result);
  }
  if(msg.method==='Runtime.exceptionThrown')errors.push(msg.params.exceptionDetails);
});
const call=(method,params={})=>new Promise((resolve,reject)=>{
  const id=++seq;
  const timer=setTimeout(()=>{pending.delete(id);reject(new Error(`CDP timeout: ${method}`));},10000);
  pending.set(id,{resolve,reject,timer});ws.send(JSON.stringify({id,method,params}));
});
const evaluate=async(expression)=>{
  const r=await call('Runtime.evaluate',{expression,returnByValue:true,awaitPromise:true});
  if(r.exceptionDetails)throw new Error(JSON.stringify(r.exceptionDetails));
  return r.result.value;
};
const wait=ms=>new Promise(r=>setTimeout(r,ms));
async function until(expression){
  for(let i=0;i<100;i++){if(await evaluate(expression))return;await wait(50);}
  throw new Error(`UI timeout: ${expression}; message=${await evaluate('document.getElementById("message").textContent')}`);
}
try {
  await call('Runtime.enable');await call('Page.enable');
  await call('Emulation.setDeviceMetricsOverride',{width:1440,height:960,deviceScaleFactor:1,mobile:false});
  await call('Page.navigate',{url});
  await until('Number(document.getElementById("points")?.textContent.replaceAll(",",""))>0');
  assert.equal(await evaluate('document.getElementById("message").textContent'),'');
  assert.equal(await evaluate('document.querySelector("canvas").getContext("webgl").getError()'),0);
  const mode=await evaluate('document.getElementById("mode").textContent');
  if(mode==='LVX2') {
    await until('!document.getElementById("restart").disabled');
    await evaluate('document.getElementById("restart").click()');
    await until('document.getElementById("frame").textContent==="0" && !document.getElementById("restart").disabled');
    // The bundled one-frame fixture also exercises EOF and restart.
    await evaluate('document.getElementById("step").click()');
    await until('document.getElementById("frame").textContent!=="0" || document.getElementById("state").textContent==="End of file"');
    await until('!document.getElementById("restart").disabled');
    await evaluate('document.getElementById("restart").click()');
    await until('document.getElementById("frame").textContent==="0" && document.getElementById("state").textContent==="Paused"');
    await evaluate('document.getElementById("play").click()');
    await until('document.getElementById("frame").textContent!=="0" || document.getElementById("state").textContent==="End of file"');
    await wait(300);
    await evaluate('if(document.getElementById("play").textContent==="Pause")document.getElementById("play").click()');
    await wait(200);
    const paused=await evaluate('document.getElementById("frame").textContent');
    await wait(150);assert.equal(await evaluate('document.getElementById("frame").textContent'),paused);
  }
  await evaluate('document.getElementById("top").click();document.getElementById("fit").click();document.querySelector(".settings").open=true;document.getElementById("color").value="1";document.getElementById("color").dispatchEvent(new Event("input"))');
  await wait(100);
  assert.equal(await evaluate('document.querySelector("canvas").getContext("webgl").getError()'),0);
  await evaluate('document.querySelector(".settings").open=false');
  const shot=await call('Page.captureScreenshot',{format:'png'});
  await writeFile('/tmp/livox-viewer-desktop.png',Buffer.from(shot.data,'base64'));
  await call('Emulation.setDeviceMetricsOverride',{width:390,height:844,deviceScaleFactor:1,mobile:true});
  await wait(100);
  assert.ok(await evaluate('document.documentElement.scrollWidth<=window.innerWidth'));
  const mobile=await call('Page.captureScreenshot',{format:'png'});
  await writeFile('/tmp/livox-viewer-mobile.png',Buffer.from(mobile.data,'base64'));
  assert.deepEqual(errors,[]);
  console.log(`Chrome PASS: ${mode} WebGL link/draw, display controls, ${mode==='LVX2'?'playback/pause/restart, ':''}desktop/mobile layout`);
} finally {
  await fetch(`${chrome}/json/close/${target.id}`);
  ws.close();
}

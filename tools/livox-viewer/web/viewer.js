const $ = (id) => document.getElementById(id);
const message = (text) => { $('message').textContent = text; $('message').classList.toggle('visible', !!text); };

function renderer(canvas) {
  const gl = canvas.getContext('webgl', { antialias: false, alpha: false });
  if (!gl) throw new Error('WebGL is unavailable. Enable browser graphics acceleration and reload.');
  const shader = (kind, source) => {
    const s = gl.createShader(kind); gl.shaderSource(s, source); gl.compileShader(s);
    if (!gl.getShaderParameter(s, gl.COMPILE_STATUS)) throw new Error(gl.getShaderInfoLog(s));
    return s;
  };
  const program = gl.createProgram();
  gl.attachShader(program, shader(gl.VERTEX_SHADER, `
    attribute vec4 a_point;
    uniform vec3 u_target; uniform vec3 u_camera; uniform float u_aspect;
    uniform float u_size; uniform float u_mode; uniform mediump float u_grid; uniform vec2 u_height;
    varying mediump vec3 v_color;
    vec3 ramp(float t) { return mix(mix(vec3(.10,.30,.72), vec3(.04,.55,.53), clamp(t*2.,0.,1.)), vec3(.81,.40,.08), clamp(t*2.-1.,0.,1.)); }
    void main() {
      float cy=cos(u_camera.x),sy=sin(u_camera.x),cp=cos(u_camera.y),sp=sin(u_camera.y);
      vec3 p=a_point.xyz-u_target;
      float x=dot(p,vec3(-sy,cy,0.));
      float y=dot(p,vec3(-cy*sp,-sy*sp,cp));
      float z=dot(p,vec3(cy*cp,sy*cp,sp))-u_camera.z;
      float nearPlane=max(.005,u_camera.z*.0001), farPlane=max(10000.,u_camera.z*10.);
      gl_Position=vec4(x*1.92/u_aspect,y*1.92,(farPlane+nearPlane)/(nearPlane-farPlane)*z+2.*farPlane*nearPlane/(nearPlane-farPlane),-z);
      gl_PointSize=u_size;
      float t=clamp((a_point.z-u_height.x)/max(.1,u_height.y-u_height.x),0.,1.);
      if (u_mode>0.5 && u_mode<1.5) t=a_point.w/255.;
      if (u_mode>1.5) t=clamp(length(a_point.xyz)/max(1.,u_camera.z),0.,1.);
      v_color=ramp(t);
      if (u_grid>0.5) {
        v_color=vec3(.87,.89,.91);
        if (a_point.w==1.) v_color=vec3(.77,.13,.12);
        if (a_point.w==2.) v_color=vec3(.09,.50,.22);
        if (a_point.w==3.) v_color=vec3(.10,.40,.82);
      }
    }`));
  gl.attachShader(program, shader(gl.FRAGMENT_SHADER, `
    precision mediump float; varying mediump vec3 v_color; uniform mediump float u_grid;
    void main(){ if(u_grid<.5 && distance(gl_PointCoord,vec2(.5))>.5) discard; gl_FragColor=vec4(v_color,1.); }
  `));
  gl.linkProgram(program);
  if (!gl.getProgramParameter(program, gl.LINK_STATUS)) throw new Error(gl.getProgramInfoLog(program));
  gl.useProgram(program);
  const u = Object.fromEntries(['target','camera','aspect','size','mode','grid','height'].map(n => [n,gl.getUniformLocation(program,`u_${n}`)]));
  const location = gl.getAttribLocation(program,'a_point');
  gl.enableVertexAttribArray(location);
  const pointBuffer = gl.createBuffer(), gridBuffer = gl.createBuffer();
  const grid = [];
  for(let n=-50;n<=50;n+=5) grid.push(-50,n,0,0,50,n,0,0,n,-50,0,0,n,50,0,0);
  grid.push(0,0,.01,1,5,0,.01,1,0,0,.01,2,0,5,.01,2,0,0,0,3,0,0,5,3);
  gl.bindBuffer(gl.ARRAY_BUFFER,gridBuffer); gl.bufferData(gl.ARRAY_BUFFER,new Float32Array(grid),gl.STATIC_DRAW);
  let points = new Float32Array(), target=[0,0,0], yaw=-.8,pitch=.55,distance=25, heights=[-2,4],fitted=false;
  function fit() {
    if(!points.length) return;
    const axes=[[],[],[]];
    const step=Math.max(1,Math.floor(points.length/4/5000));
    for(let i=0;i<points.length/4;i+=step) for(let j=0;j<3;j++) axes[j].push(points[i*4+j]);
    const bounds=axes.map(a=>{a.sort((x,y)=>x-y);return [a[Math.floor(a.length*.02)],a[Math.min(a.length-1,Math.floor(a.length*.98))]];});
    target=bounds.map(a=>(a[0]+a[1])/2);
    distance=Math.max(1,Math.hypot(...bounds.map(a=>a[1]-a[0]))*1.1);
    heights=bounds[2]; fitted=true;
  }
  let dirty=true;
  function draw() {
    if(dirty){
      dirty=false;
      const ratio=Math.min(window.devicePixelRatio||1,2);
      const w=Math.max(1,Math.round(canvas.clientWidth*ratio)),h=Math.max(1,Math.round(canvas.clientHeight*ratio));
      if(canvas.width!==w||canvas.height!==h){canvas.width=w;canvas.height=h;}
      gl.viewport(0,0,w,h); gl.clearColor(.973,.977,.984,1); gl.clear(gl.COLOR_BUFFER_BIT|gl.DEPTH_BUFFER_BIT); gl.enable(gl.DEPTH_TEST);
      gl.uniform3fv(u.target,target);gl.uniform3f(u.camera,yaw,pitch,distance);gl.uniform1f(u.aspect,w/h);
      gl.uniform1f(u.size,Number($('size').value)*ratio);gl.uniform1f(u.mode,Number($('color').value));gl.uniform2fv(u.height,heights);
      const bind=(b)=>{gl.bindBuffer(gl.ARRAY_BUFFER,b);gl.vertexAttribPointer(location,4,gl.FLOAT,false,16,0);};
      if($('grid').checked){gl.uniform1f(u.grid,1);bind(gridBuffer);gl.drawArrays(gl.LINES,0,grid.length/4);}
      gl.uniform1f(u.grid,0);bind(pointBuffer);gl.drawArrays(gl.POINTS,0,points.length/4);
    }
    requestAnimationFrame(draw);
  }
  // Allocate even for an empty initial cloud, so drawArrays has a valid buffer.
  gl.bindBuffer(gl.ARRAY_BUFFER,pointBuffer);gl.bufferData(gl.ARRAY_BUFFER,points,gl.DYNAMIC_DRAW);
  let drag=null;
  canvas.addEventListener('pointerdown',e=>{drag=[e.clientX,e.clientY];canvas.setPointerCapture(e.pointerId);});
  canvas.addEventListener('pointermove',e=>{
    if(!drag)return;
    const dx=e.clientX-drag[0],dy=e.clientY-drag[1];drag=[e.clientX,e.clientY];
    if(e.shiftKey||e.buttons===2){
      const s=distance*.0015;const right=[-Math.sin(yaw),Math.cos(yaw),0];const up=[-Math.cos(yaw)*Math.sin(pitch),-Math.sin(yaw)*Math.sin(pitch),Math.cos(pitch)];
      target=target.map((t,i)=>t-dx*s*right[i]+dy*s*up[i]);
    } else {yaw-=dx*.006;pitch=Math.max(-1.55,Math.min(1.55,pitch+dy*.006));}
    dirty=true;
  });
  const stop=()=>{drag=null;};canvas.addEventListener('pointerup',stop);canvas.addEventListener('pointercancel',stop);canvas.addEventListener('lostpointercapture',stop);
  canvas.addEventListener('contextmenu',e=>e.preventDefault());
  canvas.addEventListener('wheel',e=>{e.preventDefault();distance=Math.max(.1,Math.min(1e7,distance*Math.exp(Math.max(-100,Math.min(100,e.deltaY))*.002)));dirty=true;},{passive:false});
  $('fit').onclick=()=>{fit();dirty=true;};$('top').onclick=()=>{pitch=1.55;yaw=-Math.PI/2;dirty=true;};
  for(const id of ['color','size','grid']) $(id).addEventListener('input',()=>{dirty=true;$('size-value').textContent=`${$('size').value} px`;});
  new ResizeObserver(()=>{dirty=true;}).observe(canvas);
  canvas.addEventListener('webglcontextlost',e=>{e.preventDefault();message('Graphics context lost. Reload the viewer.');});
  draw();
  return {update(values){points=values;gl.bindBuffer(gl.ARRAY_BUFFER,pointBuffer);gl.bufferData(gl.ARRAY_BUFFER,values,gl.DYNAMIC_DRAW);if(!fitted&&points.length)fit();dirty=true;}};
}

async function main(){
  const render=renderer($('cloud'));
  const response=await fetch('/api/info');if(!response.ok)throw new Error(await response.text());
  const info=await response.json(),live=info.mode==='udp';
  $('source').textContent=info.source;$('mode').textContent=live?'Live':'LVX2';
  $('device').textContent=live?'SDK2 · first valid Cartesian sender':`ID ${info.device_id} · ${info.serial}`;
  $('history').textContent=live?`${info.window_ms} ms`:`${info.history_frames} frames`;
  $('detail').textContent=`Limit ${info.max_points.toLocaleString()} points · ${live?'Arrival-time window':`${info.frame_duration_ms} ms / frame`}`;
  $('step').hidden=live;$('restart').hidden=live;$('speed').hidden=live;
  let playing=live,busy=false,ended=false,timer;
  const label=()=>{$('play').textContent=playing?'Pause':live?'Resume':'Play';$('state').textContent=ended?'End of file':playing?(live?'Receiving':'Playing'):'Paused';$('led').classList.toggle('active',playing);};
  function display(buffer){
    const view=new DataView(buffer);if(buffer.byteLength<4)throw new Error('Incomplete viewer response');
    const n=view.getUint32(0,true),offset=4+n;
    if(offset>buffer.byteLength||(buffer.byteLength-offset)%16)throw new Error('Invalid viewer point buffer');
    const s=JSON.parse(new TextDecoder().decode(new Uint8Array(buffer,4,n)));
    const count=(buffer.byteLength-offset)/16;if(count!==s.points||count>info.max_points)throw new Error('Invalid viewer point count');
    const points=new Float32Array(count*4);
    for(let i=0;i<points.length;i++){points[i]=view.getFloat32(offset+i*4,true);if(!Number.isFinite(points[i]))throw new Error('Non-finite viewer point');}
    render.update(points);
    for(const id of ['points','packets','skipped']) $(id).textContent=s[id].toLocaleString();
    $('frame').textContent=s.frame??'—';$('invalid').textContent=`${s.invalid} / ${s.foreign}`;
    $('timestamp').textContent=s.timestamp??'—';$('clock').textContent=s.time_type?`${s.time_type} · ${s.time_type.startsWith('Unknown')?'raw':'ns'}`:'—';
    if(s.peer)$('device').textContent=`UDP sender ${s.peer}`;
    ended=s.ended;
    if(ended||s.error)playing=false;
    $('step').disabled=ended;
    message(s.error?`Read error: ${s.error}`:count?'':live?'Waiting for point packets…':'No points in this window. Play or step to continue.');
    label();
  }
  async function load(path,method='GET'){
    if(busy)return;busy=true;
    for(const id of ['step','restart']) $(id).disabled=true;
    try{const r=await fetch(path,{method});if(!r.ok)throw new Error(await r.text());display(await r.arrayBuffer());}
    catch(e){playing=false;label();message(e.message);}
    finally{busy=false;$('step').disabled=ended;$('restart').disabled=false;}
  }
  function schedule(){clearTimeout(timer);if(!playing)return;timer=setTimeout(async()=>{const start=performance.now();await load(live?'/api/cloud':'/api/next',live?'GET':'POST');if(playing){const delay=(live?50:info.frame_duration_ms/Number($('speed').value))- (performance.now()-start);timer=setTimeout(tick,Math.max(0,delay));}},0);}
  async function tick(){if(!playing)return;const start=performance.now();await load(live?'/api/cloud':'/api/next',live?'GET':'POST');if(playing)timer=setTimeout(tick,Math.max(0,(live?50:info.frame_duration_ms/Number($('speed').value))-(performance.now()-start)));}
  $('play').onclick=async()=>{
    // Pause remains available while a frame request is in flight.
    if(playing){playing=false;clearTimeout(timer);label();return;}
    if(busy)return;
    if(ended){await load('/api/reset','POST');if(ended)return;}
    playing=true;label();schedule();
  };
  $('step').onclick=async()=>{playing=false;clearTimeout(timer);await load('/api/next','POST');label();};
  $('restart').onclick=async()=>{playing=false;clearTimeout(timer);await load('/api/reset','POST');label();};
  document.addEventListener('keydown',e=>{if(e.code==='Space'&&!['INPUT','SELECT','BUTTON'].includes(document.activeElement.tagName)){e.preventDefault();$('play').click();}});
  await load(live?'/api/cloud':'/api/next',live?'GET':'POST');label();if(live)schedule();
}
main().catch(e=>message(e.message));

const {useState,useEffect,useRef,useCallback,useContext,createContext,Fragment}=React;
const html=htm.bind(React.createElement);
const DASH=location.origin;
const ROUTES=[
  {path:'/',label:'Dashboard'},
  {path:'/download',label:'Download'},
  {path:'/watch',label:'Watch'},
  {path:'/images',label:'Images'},
  {path:'/about',label:'About'}
];
const STATE_LABEL={running:'Running',unhealthy:'Unhealthy',stopped:'Stopped'};
const SOURCES={
  tiktok:{
    label:'TikTok',
    placeholder:'https://www.tiktok.com/@user/video/0000000000000000000',
    hint:'MP4 keeps the full duration. HD is the default, SD is the smaller file, MP3 is the audio track.',
    presets:[
      {id:'mp4-hd',label:'MP4, HD (default)',format:'mp4',quality:'hd'},
      {id:'mp4-sd',label:'MP4, SD',format:'mp4',quality:'sd'},
      {id:'mp3',label:'MP3 audio',format:'mp3',quality:'hd'}
    ]
  },
  youtube:{
    label:'YouTube',
    placeholder:'https://www.youtube.com/watch?v=00000000000',
    hint:'YouTube serves video and audio as separate streams. Audio downloads as m4a or opus. It does not provide MP3.',
    presets:[
      {id:'best',label:'Video, best available',format:'best'},
      {id:'1080',label:'Video, 1080p',format:'1080'},
      {id:'720',label:'Video, 720p',format:'720'},
      {id:'480',label:'Video, 480p',format:'480'},
      {id:'audio',label:'Audio only (m4a or opus)',format:'audio'}
    ]
  }
};
const NavCtx=createContext(()=>{});
const ToastCtx=createContext(()=>{});

function svc(port,path){
  return location.protocol+'//'+location.hostname+':'+port+path;
}
function cleanPath(p){
  if(!p||p==='/') return '/';
  return p.length>1&&p.endsWith('/')?p.slice(0,-1):p;
}
function titleFor(p){
  const hit=ROUTES.find(r=>r.path===p);
  return hit?hit.label:'Not found';
}
function fmtBytes(n){
  if(n===null||n===undefined||isNaN(n)) return '';
  if(n<1024) return n+' B';
  const units=['KB','MB','GB'];
  let v=n,i=-1;
  do{v=v/1024;i++;}while(v>=1024&&i<units.length-1);
  return (v>=10?v.toFixed(0):v.toFixed(1))+' '+units[i];
}
function fmtUptime(s){
  if(!s||s<1) return 'just now';
  const h=Math.floor(s/3600);
  const m=Math.floor((s%3600)/60);
  const sec=s%60;
  if(h>0) return h+'h '+String(m).padStart(2,'0')+'m';
  if(m>0) return m+'m '+String(sec).padStart(2,'0')+'s';
  return sec+'s';
}
function sameList(a,b){
  if(!a||!b||a.length!==b.length) return false;
  for(let i=0;i<a.length;i++){
    const x=a[i],y=b[i];
    if(x.name!==y.name||x.port!==y.port||x.pid!==y.pid||x.state!==y.state||x.uptime!==y.uptime||x.endpoints!==y.endpoints) return false;
  }
  return true;
}
function parseId(v){
  const s=(v||'').trim();
  const m=s.match(/(?:v=|youtu\.be\/|shorts\/|embed\/|live\/)([A-Za-z0-9_-]{11})/);
  if(m) return m[1];
  return /^[A-Za-z0-9_-]{11}$/.test(s)?s:'';
}
function parseFilename(cd,fallback){
  if(!cd) return fallback;
  const star=cd.match(/filename\*=UTF-8''([^;]+)/i);
  if(star){
    try{return decodeURIComponent(star[1].replace(/^"|"$/g,''));}catch(e){}
  }
  const plain=cd.match(/filename="?([^";]+)"?/i);
  return plain?plain[1]:fallback;
}
async function errorFrom(res){
  let msg='Request failed ('+res.status+')';
  try{
    const body=await res.json();
    if(body&&body.error) msg=body.error;
  }catch(e){}
  return msg;
}
async function getJson(url,init){
  const res=await fetch(url,Object.assign({cache:'no-store'},init||{}));
  if(!res.ok) throw new Error(await errorFrom(res));
  return res.json();
}
function saveBlob(blob,name){
  const url=URL.createObjectURL(blob);
  const a=document.createElement('a');
  a.href=url;
  a.download=name;
  document.body.appendChild(a);
  a.click();
  a.remove();
  setTimeout(()=>URL.revokeObjectURL(url),10000);
}

function usePath(){
  const [path,setPath]=useState(()=>cleanPath(location.pathname));
  useEffect(()=>{
    const onPop=()=>setPath(cleanPath(location.pathname));
    window.addEventListener('popstate',onPop);
    return ()=>window.removeEventListener('popstate',onPop);
  },[]);
  const go=useCallback(to=>{
    const next=cleanPath(to);
    if(next===cleanPath(location.pathname)){
      window.scrollTo(0,0);
      return;
    }
    history.pushState(null,'',next);
    setPath(next);
    window.scrollTo(0,0);
  },[]);
  return [path,go];
}
function useServices(){
  const [data,setData]=useState(null);
  const [err,setErr]=useState('');
  const alive=useRef(false);
  const load=useCallback(async()=>{
    try{
      const next=await getJson(DASH+'/api/services');
      if(!alive.current) return;
      setData(prev=>sameList(prev,next)?prev:next);
      setErr('');
    }catch(e){
      if(alive.current) setErr(e.message);
    }
  },[]);
  useEffect(()=>{
    alive.current=true;
    let timer=null;
    async function tick(){
      if(!document.hidden) await load();
      if(alive.current) timer=setTimeout(tick,2000);
    }
    tick();
    const onVis=()=>{
      if(!document.hidden&&alive.current){
        if(timer) clearTimeout(timer);
        tick();
      }
    };
    document.addEventListener('visibilitychange',onVis);
    return ()=>{
      alive.current=false;
      if(timer) clearTimeout(timer);
      document.removeEventListener('visibilitychange',onVis);
    };
  },[load]);
  return {data,err,reload:load};
}

function Link({to,className,children,current}){
  const go=useContext(NavCtx);
  return html`<a href=${to} className=${className||''} aria-current=${current?'page':undefined} onClick=${e=>{
    if(e.button!==0||e.metaKey||e.ctrlKey||e.shiftKey||e.altKey) return;
    e.preventDefault();
    go(to);
  }}>${children}</a>`;
}
function PageHead({title,sub,children}){
  return html`<div className="page-head">
    <div>
      <h1>${title}</h1>
      ${sub&&html`<p>${sub}</p>`}
    </div>
    ${children&&html`<div className="page-actions">${children}</div>`}
  </div>`;
}
function Alert({kind,children}){
  return html`<div className=${'note '+(kind||'info')} role=${kind==='err'?'alert':'status'}>${children}</div>`;
}
function Spinner(){
  return html`<span className="spinner" aria-hidden="true"></span>`;
}
function OptInput({k,label,opts,setOpts,type,placeholder,min,max,step}){
  return html`<div className="field">
    <label htmlFor=${'opt-'+k}>${label}</label>
    <input id=${'opt-'+k} className="input" type=${type||'number'} value=${opts[k]===undefined?'':opts[k]}
      placeholder=${placeholder||''} min=${min} max=${max} step=${step}
      onChange=${e=>setOpts(o=>Object.assign({},o,{[k]:e.target.value}))} />
  </div>`;
}

function LogBox({name}){
  const [text,setText]=useState('');
  const [err,setErr]=useState('');
  const box=useRef(null);
  const stick=useRef(true);
  useEffect(()=>{
    let alive=true;
    let timer=null;
    async function load(){
      if(!document.hidden){
        try{
          const body=await getJson(DASH+'/api/logs?name='+encodeURIComponent(name));
          if(!alive) return;
          setText(prev=>prev===body.log?prev:body.log);
          setErr('');
        }catch(e){
          if(alive) setErr(e.message);
        }
      }
      if(alive) timer=setTimeout(load,2000);
    }
    load();
    return ()=>{
      alive=false;
      if(timer) clearTimeout(timer);
    };
  },[name]);
  useEffect(()=>{
    const el=box.current;
    if(el&&stick.current) el.scrollTop=el.scrollHeight;
  },[text]);
  return html`<pre className="logbox" ref=${box} onScroll=${e=>{
    const el=e.currentTarget;
    stick.current=el.scrollHeight-el.scrollTop-el.clientHeight<24;
  }}>${err?'Log unavailable: '+err:(text||'No output yet.')}</pre>`;
}

function ServiceRow({s,onDone}){
  const notify=useContext(ToastCtx);
  const [busy,setBusy]=useState('');
  const [open,setOpen]=useState(false);
  async function act(op){
    setBusy(op);
    try{
      const res=await fetch(DASH+'/api/'+op+'?name='+encodeURIComponent(s.name),{method:'POST',cache:'no-store'});
      if(!res.ok) throw new Error(await errorFrom(res));
      notify(s.name+(op==='start'?' started':' stopped'),'ok');
    }catch(e){
      notify(s.name+': '+e.message,'err');
    }finally{
      setBusy('');
      onDone();
    }
  }
  const eps=String(s.endpoints||'').split(', ').filter(Boolean);
  const canStart=s.state!=='running';
  const canStop=s.state!=='stopped';
  return html`<article className="svc" data-state=${s.state}>
    <div className="svc-main">
      <div className="svc-head">
        <span className="svc-name">${s.name}</span>
        <span className="svc-port">:${s.port}</span>
        <span className=${'state s-'+s.state}>${STATE_LABEL[s.state]||s.state}</span>
      </div>
      <div className="svc-meta">
        <span>PID <b>${s.pid===null?'-':s.pid}</b></span>
        <span>Uptime <b>${fmtUptime(s.uptime)}</b></span>
      </div>
      <div className="eps">${eps.map(ep=>html`<span className="ep" key=${ep}>${ep}</span>`)}</div>
      <details className="log" onToggle=${e=>setOpen(e.currentTarget.open)}>
        <summary>${open?'Hide log':'Show log'}</summary>
        ${open&&html`<${LogBox} name=${s.name} />`}
      </details>
    </div>
    <div className="svc-actions">
      <button type="button" className="btn primary" disabled=${busy!==''||!canStart} onClick=${()=>act('start')}>${busy==='start'?'Starting':'Start'}</button>
      <button type="button" className="btn stop" disabled=${busy!==''||!canStop} onClick=${()=>act('stop')}>${busy==='stop'?'Stopping':'Stop'}</button>
    </div>
  </article>`;
}

function Dashboard(){
  const {data,err,reload}=useServices();
  const running=data?data.filter(s=>s.state==='running').length:0;
  return html`<${Fragment}>
    <${PageHead} title="Services" sub="Start, stop, and watch the four local APIs." />
    <div className="statusbar">
      <span className="livebox">
        <span className=${'pulse'+(running>0?' on':'')} aria-hidden="true"></span>
        <strong>${err&&!data?'Dashboard API unreachable':data?running+' of '+data.length+' running':'Checking services'}</strong>
      </span>
      <span className="sb-meta">
        <span>Dashboard <code>${location.host}</code></span>
        <span>Auto refresh 2s</span>
        <button type="button" className="btn ghost sm" onClick=${reload}>Refresh now</button>
      </span>
    </div>
    ${err&&data&&html`<${Alert} kind="err"><span>Live updates paused: ${err}</span><button type="button" className="btn ghost sm" onClick=${reload}>Retry</button></${Alert}>`}
    ${err&&!data&&html`<div className="panel">
      <${Alert} kind="err"><span>Cannot reach the dashboard API: ${err}</span><button type="button" className="btn ghost sm" onClick=${reload}>Retry</button></${Alert}>
      <p>This page talks to the dashboard on its own port. Check that the process is still running.</p>
    </div>`}
    <div className="svc-list" aria-busy=${!data}>
      ${!data
        ? (err?null:[0,1,2,3].map(i=>html`<div className="skel" key=${i}></div>`))
        : data.map(s=>html`<${ServiceRow} key=${s.name} s=${s} onDone=${reload} />`)}
    </div>
  </${Fragment}>`;
}

function Download(){
  const [source,setSource]=useState('tiktok');
  const [url,setUrl]=useState('');
  const [preset,setPreset]=useState(SOURCES.tiktok.presets[0].id);
  const [busy,setBusy]=useState(false);
  const [pct,setPct]=useState(0);
  const [note,setNote]=useState('');
  const [err,setErr]=useState('');
  const current=SOURCES[source];
  const p=current.presets.find(x=>x.id===preset)||current.presets[0];
  function pick(id){
    setSource(id);
    setPreset(SOURCES[id].presets[0].id);
    setErr('');
    setNote('');
    setPct(0);
  }
  async function submit(e){
    if(e) e.preventDefault();
    if(busy) return;
    const raw=url.trim();
    if(!raw){
      setErr('Paste a link first.');
      return;
    }
    setErr('');
    setNote('');
    setPct(0);
    setBusy(true);
    const target=source==='tiktok'
      ? svc(8082,'/download?url='+encodeURIComponent(raw)+'&format='+p.format+'&quality='+p.quality)
      : svc(8085,'/api/download?v='+encodeURIComponent(raw)+'&format='+p.format);
    try{
      const res=await fetch(target,{cache:'no-store'});
      if(!res.ok) throw new Error(await errorFrom(res));
      const total=Number(res.headers.get('content-length')||0);
      let blob;
      if(res.body&&res.body.getReader){
        const reader=res.body.getReader();
        const parts=[];
        let got=0;
        while(true){
          const step=await reader.read();
          if(step.done) break;
          if(step.value){
            parts.push(step.value);
            got+=step.value.length;
            if(total) setPct(Math.min(99,Math.round(got/total*100)));
            else setNote('Received '+fmtBytes(got));
          }
        }
        blob=new Blob(parts,{type:res.headers.get('content-type')||'application/octet-stream'});
      }else{
        blob=await res.blob();
      }
      setPct(100);
      const name=parseFilename(res.headers.get('content-disposition'),source==='tiktok'?'tiktok.'+(p.format==='mp3'?'mp3':'mp4'):'youtube.mp4');
      saveBlob(blob,name);
      setNote('Saved '+name+' ('+fmtBytes(blob.size)+')');
    }catch(e2){
      setErr(e2.message);
      setPct(0);
    }finally{
      setBusy(false);
    }
  }
  return html`<${Fragment}>
    <${PageHead} title="Download" sub="Fetch a TikTok or YouTube file through the local APIs." />
    <div className="panel">
      <div className="seg" role="radiogroup" aria-label="Source">
        ${['tiktok','youtube'].map(id=>html`<label key=${id}>
          <input type="radio" name="source" value=${id} checked=${source===id} onChange=${()=>pick(id)} />
          <span>${SOURCES[id].label}</span>
        </label>`)}
      </div>
      <form onSubmit=${submit}>
        <div className="form-row">
          <div className="field">
            <label htmlFor="dl-url">Video link</label>
            <input id="dl-url" className="input" type="url" value=${url} placeholder=${current.placeholder}
              autoComplete="off" spellCheck="false" disabled=${busy} onInput=${e=>setUrl(e.target.value)} />
          </div>
          <div className="field narrow">
            <label htmlFor="dl-preset">Download</label>
            <select id="dl-preset" className="input" value=${p.id} disabled=${busy} onChange=${e=>setPreset(e.target.value)}>
              ${current.presets.map(x=>html`<option key=${x.id} value=${x.id}>${x.label}</option>`)}
            </select>
          </div>
        </div>
        <div className="actions-row">
          <button type="submit" className="btn primary" disabled=${busy}>${busy?html`<${Fragment}><${Spinner} /> Fetching</${Fragment}>`:'Fetch file'}</button>
          <span className="hint" role="status">${busy?'Working. Long videos can take a while.':note}</span>
        </div>
        ${busy&&html`<div className="progress" role="progressbar" aria-valuemin="0" aria-valuemax="100" aria-valuenow=${pct}><i style=${{width:pct+'%'}}></i></div>`}
      </form>
      ${err&&html`<${Alert} kind="err"><span>${err}</span></${Alert}>`}
      <div className="actions-row"><span className="hint">${current.hint}</span></div>
    </div>
  </${Fragment}>`;
}

function Watch(){
  const [input,setInput]=useState('');
  const [src,setSrc]=useState('');
  const [state,setState]=useState('idle');
  const [err,setErr]=useState('');
  const [query,setQuery]=useState('');
  const [results,setResults]=useState(null);
  const [searchState,setSearchState]=useState('idle');
  const player=useRef(null);
  function playId(id){
    if(!id) return;
    setInput('https://www.youtube.com/watch?v='+id);
    setErr('');
    setState('loading');
    setSrc(svc(8085,'/api/stream?v='+encodeURIComponent(id)+'&format=best'));
  }
  function submit(e){
    if(e) e.preventDefault();
    const id=parseId(input);
    if(!id){
      setErr('Enter a YouTube video ID or link.');
      setState('error');
      return;
    }
    playId(id);
  }
  async function search(e){
    if(e) e.preventDefault();
    const q=query.trim();
    if(!q){
      setErr('Type a word or phrase to search.');
      return;
    }
    setErr('');
    setResults(null);
    setSearchState('loading');
    try{
      const body=await getJson(svc(8085,'/api/search?q='+encodeURIComponent(q)));
      setResults(body.results||[]);
      setSearchState('done');
    }catch(e2){
      setErr(e2.message);
      setSearchState('error');
    }
  }
  useEffect(()=>{
    if(!src) return;
    const v=player.current;
    if(!v) return;
    const p=v.play();
    if(p&&p.catch) p.catch(()=>{});
  },[src]);
  return html`<${Fragment}>
    <${PageHead} title="Watch" sub="Search YouTube or paste a link, then play the stream here." />
    <div className="panel">
      <form onSubmit=${submit}>
        <div className="form-row">
          <div className="field">
            <label htmlFor="yt-input">YouTube link or video ID</label>
            <input id="yt-input" className="input" type="text" value=${input} placeholder="https://www.youtube.com/watch?v=..."
              autoComplete="off" spellCheck="false" onInput=${e=>setInput(e.target.value)} />
          </div>
          <button type="submit" className="btn primary">Play stream</button>
        </div>
      </form>
      ${err&&html`<${Alert} kind="err"><span>${err}</span></${Alert}>`}
      <div className="actions-row">
        ${src
          ? html`<video key=${src} ref=${player} className="player" controls playsInline preload="metadata" src=${src}
              onCanPlay=${()=>setState('ready')}
              onError=${()=>{setState('error');setErr('The stream did not start. Make sure youtubeapi is running on port 8085.');}}></video>`
          : html`<div className="player-empty">
              <span className="drop-icon">YT</span>
              <strong>No stream loaded</strong>
              <span className="hint">Search below or paste a link above to play the video stream here.</span>
            </div>`}
      </div>
      <p className="hint" role="status">${state==='loading'?'Loading stream...':state==='ready'?'Playing. Use the player controls for fullscreen and scrubbing.':''}</p>
    </div>
    <div className="panel" style=${{marginTop:'14px'}}>
      <h2>Search YouTube</h2>
      <p>Results come from the local YouTube API. Pick one to play it above.</p>
      <form className="search-row" onSubmit=${search}>
        <div className="field">
          <label htmlFor="yt-search">Search term</label>
          <input id="yt-search" className="input" type="search" value=${query} placeholder="Try a song, topic, or channel"
            autoComplete="off" onInput=${e=>setQuery(e.target.value)} />
        </div>
        <button type="submit" className="btn stop" disabled=${searchState==='loading'}>${searchState==='loading'?html`<${Fragment}><${Spinner} /> Searching</${Fragment}>`:'Search'}</button>
      </form>
      ${searchState==='idle'&&html`<div className="search-state">Search results appear here.</div>`}
      ${searchState==='loading'&&html`<div className="search-results" aria-busy="true">${[0,1,2,3].map(i=>html`<div className="skel" key=${i}></div>`)}</div>`}
      ${searchState==='error'&&html`<div className="search-state">Search failed. Check that youtubeapi is running on port 8085.</div>`}
      ${searchState==='done'&&results&&html`<${Fragment}>
        <div className="search-head">
          <h3>${results.length} result${results.length===1?'':'s'}</h3>
          <span className="hint">Click a card to play it in the player above.</span>
        </div>
        ${results.length===0
          ? html`<div className="search-state">No results for this search. Try different words.</div>`
          : html`<div className="search-results">
              ${results.map(v=>html`<button type="button" className="vcard" key=${v.id} onClick=${()=>playId(v.id)}>
                <span className="vthumb">
                  ${v.thumbnail&&html`<img src=${v.thumbnail} alt="" loading="lazy" />`}
                  ${v.duration&&html`<span className="vdur">${v.duration}</span>`}
                </span>
                <span className="vmeta">
                  <span className="vtitle">${v.title}</span>
                  <span className="vauthor">${v.author}${v.verified?' ✓':''}${v.live?html`<span className="vtag">LIVE</span>`:''}</span>
                  <span className="vstats">${[v.views,v.published].filter(Boolean).join(' · ')}</span>
                </span>
              </button>`)}
            </div>`}
      </${Fragment}>`}
      <div className="actions-row"><span className="hint">YouTube sends video and audio as separate streams. The player shows video only. Use Download for the audio track.</span></div>
    </div>
  </${Fragment}>`;
}

function Images(){
  const [action,setAction]=useState('bg');
  const [file,setFile]=useState(null);
  const [inUrl,setInUrl]=useState('');
  const [outUrl,setOutUrl]=useState('');
  const [inDims,setInDims]=useState('');
  const [outDims,setOutDims]=useState('');
  const [outSize,setOutSize]=useState(0);
  const [opts,setOpts]=useState({threshold:'32',color:''});
  const [busy,setBusy]=useState(false);
  const [over,setOver]=useState(false);
  const [status,setStatus]=useState('');
  const [err,setErr]=useState('');
  const inRef=useRef('');
  const outRef=useRef('');
  const cfg=action==='bg'
    ? {label:'Remove background',port:8084,path:'/remove-bg',defaults:{threshold:'32',color:''}}
    : {label:'Remove watermark',port:8081,path:'/remove-watermark',defaults:{threshold:'200',radius:'3',iters:'4',x:'',y:'',w:'',h:''}};
  useEffect(()=>{
    setOpts(cfg.defaults);
    setErr('');
    setStatus('');
    const old=outRef.current;
    outRef.current='';
    setOutUrl('');
    setOutDims('');
    setOutSize(0);
    if(old) setTimeout(()=>URL.revokeObjectURL(old),0);
  },[action]);
  useEffect(()=>()=>{
    if(inRef.current) URL.revokeObjectURL(inRef.current);
    if(outRef.current) URL.revokeObjectURL(outRef.current);
  },[]);
  function clearOut(){
    const old=outRef.current;
    outRef.current='';
    setOutUrl('');
    setOutDims('');
    setOutSize(0);
    if(old) setTimeout(()=>URL.revokeObjectURL(old),0);
  }
  function choose(f){
    if(!f) return;
    const isPng=f.type==='image/png'||/\.png$/i.test(f.name);
    if(!isPng){
      setErr('Choose a PNG file. The image APIs read 8-bit PNG only.');
      setStatus('');
      return;
    }
    const old=inRef.current;
    const next=URL.createObjectURL(f);
    inRef.current=next;
    setInUrl(next);
    setInDims('');
    setFile(f);
    clearOut();
    setErr('');
    setStatus(f.name+' ('+fmtBytes(f.size)+')');
    if(old) setTimeout(()=>URL.revokeObjectURL(old),0);
  }
  async function run(e){
    if(e) e.preventDefault();
    if(busy) return;
    if(!file){
      setErr('Choose a PNG file first.');
      return;
    }
    setBusy(true);
    setErr('');
    clearOut();
    const q=new URLSearchParams();
    Object.keys(opts).forEach(k=>{
      const v=String(opts[k]).trim();
      if(v!=='') q.set(k,v);
    });
    q.set('_',String(Date.now()));
    try{
      const form=new FormData();
      form.append('image',file,file.name);
      const res=await fetch(svc(cfg.port,cfg.path)+'?'+q.toString(),{method:'POST',body:form,cache:'no-store'});
      if(!res.ok) throw new Error(await errorFrom(res));
      const blob=await res.blob();
      if(!blob.size) throw new Error('The API returned an empty image.');
      const url=URL.createObjectURL(blob);
      outRef.current=url;
      setOutUrl(url);
      setOutSize(blob.size);
      setStatus('Result ready.');
    }catch(e2){
      setErr(e2.message);
      setStatus('');
    }finally{
      setBusy(false);
    }
  }
  return html`<${Fragment}>
    <${PageHead} title="Images" sub="Remove a background or a watermark from a PNG." />
    <div className="panel">
      <div className="seg" role="radiogroup" aria-label="Action">
        <label>
          <input type="radio" name="action" value="bg" checked=${action==='bg'} onChange=${()=>setAction('bg')} />
          <span>Remove background</span>
        </label>
        <label>
          <input type="radio" name="action" value="wm" checked=${action==='wm'} onChange=${()=>setAction('wm')} />
          <span>Remove watermark</span>
        </label>
      </div>
      <form onSubmit=${run}>
        <label className=${'drop'+(over?' over':'')}
          onDragOver=${e=>{e.preventDefault();setOver(true);}}
          onDragLeave=${e=>{if(!e.currentTarget.contains(e.relatedTarget)) setOver(false);}}
          onDrop=${e=>{e.preventDefault();setOver(false);choose(e.dataTransfer.files[0]);}}>
          <input type="file" accept="image/png,.png" onChange=${e=>{choose(e.target.files[0]);e.target.value='';}} />
          <span className="drop-icon">PNG</span>
          <strong>${file?'Choose another image':'Choose a PNG or drop it here'}</strong>
          <span className="hint">Only 8-bit PNG is supported.</span>
        </label>
        ${action==='bg'
          ? html`<${Fragment}>
            <div className="form-grid" style=${{marginTop:'14px'}}>
              <${OptInput} k="threshold" label="Color threshold" opts=${opts} setOpts=${setOpts} min="0" max="255" />
              <${OptInput} k="color" label="Background color (optional)" opts=${opts} setOpts=${setOpts} type="text" placeholder="auto from corner" />
            </div>
            <p className="hint" style=${{marginTop:'8px'}}>The corner color is detected when no color is set.</p>
          </${Fragment}>`
          : html`<${Fragment}>
            <div className="form-grid" style=${{marginTop:'14px'}}>
              <${OptInput} k="x" label="Box X" opts=${opts} setOpts=${setOpts} min="0" placeholder="auto" />
              <${OptInput} k="y" label="Box Y" opts=${opts} setOpts=${setOpts} min="0" placeholder="auto" />
              <${OptInput} k="w" label="Box width" opts=${opts} setOpts=${setOpts} min="1" placeholder="auto" />
              <${OptInput} k="h" label="Box height" opts=${opts} setOpts=${setOpts} min="1" placeholder="auto" />
              <${OptInput} k="threshold" label="Brightness threshold" opts=${opts} setOpts=${setOpts} min="0" max="255" />
              <${OptInput} k="radius" label="Inpaint radius" opts=${opts} setOpts=${setOpts} min="1" max="20" />
              <${OptInput} k="iters" label="Inpaint passes" opts=${opts} setOpts=${setOpts} min="1" max="20" />
            </div>
            <p className="hint" style=${{marginTop:'8px'}}>Leave the box empty to detect bright watermark pixels automatically.</p>
          </${Fragment}>`}
        <div className="actions-row">
          <button type="submit" className="btn primary" disabled=${busy}>${busy?html`<${Fragment}><${Spinner} /> Processing</${Fragment}>`:'Process image'}</button>
          <span className="hint" role="status">${status}</span>
        </div>
      </form>
      ${err&&html`<${Alert} kind="err"><span>${err}</span></${Alert}>`}
      <div className="preview">
        <figure className="shot">
          <figcaption>Input</figcaption>
          <div className="thumb">
            ${inUrl
              ? html`<img src=${inUrl} alt="Selected input image" onLoad=${e=>setInDims(e.currentTarget.naturalWidth+' x '+e.currentTarget.naturalHeight)} />`
              : html`<span className="thumb-empty">No file selected</span>`}
          </div>
        </figure>
        <figure className="shot">
          <figcaption>Result</figcaption>
          <div className="thumb" aria-busy=${busy}>
            ${busy
              ? html`<span className="thumb-empty"><${Spinner} />Processing</span>`
              : outUrl
                ? html`<img src=${outUrl} alt="Processed result" onLoad=${e=>setOutDims(e.currentTarget.naturalWidth+' x '+e.currentTarget.naturalHeight)} />`
                : html`<span className="thumb-empty">Result appears here</span>`}
          </div>
        </figure>
      </div>
      <div className="actions-row">
        ${outUrl
          ? html`<a className="btn primary" href=${outUrl} download=${'result-'+action+'.png'}>Download result</a><span className="hint">${[outDims,fmtBytes(outSize)].filter(Boolean).join(' · ')}</span>`
          : html`<span className="hint">${inDims?'Input size '+inDims:''}</span>`}
      </div>
    </div>
  </${Fragment}>`;
}

function About(){
  const {data,err,reload}=useServices();
  return html`<${Fragment}>
    <${PageHead} title="About" sub="What this project is, who made it, and what runs behind the dashboard." />
    <div className="about-hero">
      <section className="identity">
        <span className="kicker">Local REST API collection</span>
        <h2>local-restapi</h2>
        <p>Four small APIs for video, audio, and images, plus one browser dashboard to run them. Everything runs on this device with no cloud service and no Node.js in the build or runtime.</p>
        <div className="identity-links">
          <a href="https://github.com/Kandoraq/local-restapi" target="_blank" rel="noreferrer">Repository</a>
          <a href="https://github.com/Kandoraq" target="_blank" rel="noreferrer">Developer profile</a>
        </div>
      </section>
      <div className="about-side">
        <section className="panel">
          <h3>Developer</h3>
          <p>Built and maintained by Kandoraq.</p>
          <span className="handle">@Kandoraq</span>
          <div className="actions-row">
            <a className="btn ghost sm" href="https://github.com/Kandoraq" target="_blank" rel="noreferrer">View GitHub profile</a>
            <a className="btn ghost sm" href="https://github.com/Kandoraq?tab=repositories" target="_blank" rel="noreferrer">All repositories</a>
          </div>
        </section>
        <section className="panel">
          <h3>Project details</h3>
          <p>Source, issues, and releases live on GitHub. The code is MIT licensed.</p>
          <div className="actions-row">
            <a className="btn ghost sm" href="https://github.com/Kandoraq/local-restapi" target="_blank" rel="noreferrer">Repository</a>
            <a className="btn ghost sm" href="https://github.com/Kandoraq/local-restapi/blob/main/LICENSE" target="_blank" rel="noreferrer">MIT license</a>
            <a className="btn ghost sm" href="https://github.com/Kandoraq/local-restapi/issues" target="_blank" rel="noreferrer">Issues</a>
          </div>
          <div className="stack-list"><span>Rust</span><span>React 18</span><span>No Node.js</span><span>Private repo</span></div>
        </section>
      </div>
    </div>
    <section className="panel">
      <div className="page-head" style=${{marginBottom:'12px'}}>
        <div>
          <h2>Running services</h2>
          <p>Fetched live from the dashboard. Endpoints below are the ones each service exposes.</p>
        </div>
        <button type="button" className="btn ghost sm" onClick=${reload}>Refresh</button>
      </div>
      ${err&&!data&&html`<${Alert} kind="err"><span>Cannot load service data: ${err}</span></${Alert}>`}
      ${!data&&!err&&html`<div className="search-state">Loading services...</div>`}
      ${data&&html`<div style=${{overflowX:'auto'}}>
        <table className="svc-table">
          <thead><tr><th>Service</th><th>Port</th><th>Status</th><th>PID</th><th>Endpoints</th></tr></thead>
          <tbody>
            ${data.map(s=>html`<tr key=${s.name}>
              <td><code>${s.name}</code></td>
              <td><code>${s.port}</code></td>
              <td><span className=${'dot '+(s.state==='running'?'on':s.state==='unhealthy'?'warn':'')}></span>${STATE_LABEL[s.state]||s.state}</td>
              <td><code>${s.pid===null?'-':s.pid}</code></td>
              <td><code>${s.endpoints}</code></td>
            </tr>`)}
          </tbody>
        </table>
      </div>`}
    </section>
  </${Fragment}>`;
}

function NotFound(){
  return html`<${Fragment}>
    <${PageHead} title="Page not found" sub="That address does not match any page in this dashboard." />
    <${Link} to="/" className="btn primary">Back to dashboard</${Link}>
  </${Fragment}>`;
}

function App(){
  const [path,go]=usePath();
  const [toast,setToast]=useState(null);
  const notify=useCallback((msg,kind)=>{
    setToast({msg,kind:kind||'ok',id:Date.now()});
  },[]);
  useEffect(()=>{
    if(!toast) return;
    const t=setTimeout(()=>setToast(null),4200);
    return ()=>clearTimeout(t);
  },[toast]);
  useEffect(()=>{
    document.title=titleFor(path)+' - local-restapi';
  },[path]);
  let page;
  if(path==='/') page=html`<${Dashboard} />`;
  else if(path==='/download') page=html`<${Download} />`;
  else if(path==='/watch') page=html`<${Watch} />`;
  else if(path==='/images') page=html`<${Images} />`;
  else if(path==='/about') page=html`<${About} />`;
  else page=html`<${NotFound} />`;
  return html`<${NavCtx.Provider} value=${go}>
    <${ToastCtx.Provider} value=${notify}>
      <header className="top">
        <div className="top-in">
          <${Link} to="/" className="brand">
            <span className="mark" aria-hidden="true"><i></i><i></i><i></i></span>
            <span className="word">local-restapi</span>
          </${Link}>
          <nav className="nav" aria-label="Main">
            ${ROUTES.map(r=>html`<${Link} key=${r.path} to=${r.path} current=${r.path===path} className=${'navlink'+(r.path===path?' on':'')}>${r.label}</${Link}>`)}
          </nav>
        </div>
      </header>
      <main id="main" className="page">${page}</main>
      <footer className="foot">
        <span>local-restapi</span>
        <span>MIT licensed</span>
        <a href="https://github.com/Kandoraq/local-restapi" target="_blank" rel="noreferrer">GitHub</a>
        <span className="sep">Dashboard on <code>${location.host}</code></span>
      </footer>
      ${toast&&html`<div key=${toast.id} className=${'toast '+toast.kind} role=${toast.kind==='err'?'alert':'status'}>${toast.msg}</div>`}
    </${ToastCtx.Provider}>
  </${NavCtx.Provider}>`;
}

ReactDOM.createRoot(document.getElementById('root')).render(html`<${App} />`);

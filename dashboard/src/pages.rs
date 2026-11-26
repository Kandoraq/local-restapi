const CSS: &str = "
:root{--bg:#f7f8fa;--panel:#fff;--text:#1f2328;--muted:#656d76;--line:#d0d7de;--accent:#0969da;--ok:#1a7f37;--bad:#cf222e}
*{box-sizing:border-box}
body{margin:0;font:14px/1.5 -apple-system,BlinkMacSystemFont,'Segoe UI',sans-serif;color:var(--text);background:var(--bg)}
.wrap{display:flex;min-height:100vh}
.side{width:200px;background:var(--panel);border-right:1px solid var(--line);padding:20px 16px}
.side h1{font-size:16px;margin:0 0 20px}
.side a{display:block;padding:8px 10px;color:var(--text);text-decoration:none;border-radius:6px;margin-bottom:4px}
.side a:hover,.side a.on{background:#eaeef2;color:var(--accent)}
.main{flex:1;padding:28px 32px;max-width:1000px}
h2{margin:0 0 18px;font-size:22px}
.card{background:var(--panel);border:1px solid var(--line);border-radius:8px;padding:16px;margin-bottom:14px}
.grid{display:grid;grid-template-columns:repeat(auto-fit,minmax(280px,1fr));gap:14px}
.row{display:flex;align-items:center;gap:10px;flex-wrap:wrap}
.muted{color:var(--muted)}
.ok{color:var(--ok);font-weight:600}
.bad{color:var(--bad);font-weight:600}
.warn{color:#9a6700;font-weight:600}
button{background:var(--accent);color:#fff;border:0;border-radius:6px;padding:7px 14px;cursor:pointer;font-size:14px}
button.secondary{background:#eaeef2;color:var(--text)}
input,select{background:#fff;border:1px solid var(--line);border-radius:6px;padding:7px 10px;font-size:14px}
input[type=file]{padding:6px}
pre{background:#0d1117;color:#c9d1d9;border-radius:6px;padding:10px;overflow:auto;max-height:180px;font-size:12px}
img{max-width:100%;border:1px solid var(--line);border-radius:6px}
video{width:100%;max-width:800px;background:#000;border-radius:6px}
label{display:inline-block;min-width:110px;color:var(--muted)}
a{color:var(--accent)}
";

fn layout(active: &str, title: &str, body: &str) -> String {
    let nav = [
        ("dashboard", "/", "Dashboard"),
        ("download", "/download", "Download"),
        ("watch", "/watch", "Watch"),
        ("images", "/images", "Images"),
        ("about", "/about", "About"),
    ];
    let mut links = String::new();
    for (key, href, label) in nav {
        let on = if key == active { " class='on'" } else { "" };
        links.push_str(&format!("<a href='{}'{}>{}</a>", href, on, label));
    }
    format!(
        "<!doctype html><html lang='en'><head><meta charset='utf-8'><meta name='viewport' content='width=device-width,initial-scale=1'><title>{} - local-restapi</title><style>{}</style></head><body><div class='wrap'><aside class='side'><h1>local-restapi</h1>{}</aside><main class='main'><h2>{}</h2>{}</main></div></body></html>",
        title, CSS, links, title, body
    )
}

pub fn dashboard(host: &str) -> String {
    let body = format!(
        "<div id='cards' class='grid'>Loading...</div>
<script>
const API='http://{}:8080';
function render(items){{
 document.getElementById('cards').innerHTML=items.map(s=>{{
  const state=s.state==='running'?'<span class=ok>Running</span>':s.state==='unhealthy'?'<span class=warn>Unhealthy</span>':'<span class=bad>Stopped</span>';
  const pid=s.pid===null?'-':s.pid;
  return `<div class=card><div class=row><strong>${{s.name}}</strong><span class=muted>port ${{s.port}}</span>${{state}}</div>
  <div class=muted>PID ${{pid}} / uptime ${{s.uptime}}s</div>
  <div class=muted>${{s.endpoints}}</div>
  <pre id='log-${{s.name}}'></pre>
  <div class=row><button onclick=\"act('${{s.name}}','start')\">Start</button><button class=secondary onclick=\"act('${{s.name}}','stop')\">Stop</button></div></div>`;
 }}).join('');
 items.forEach(s=>fetch(API+'/api/logs?name='+s.name).then(r=>r.json()).then(j=>{{const e=document.getElementById('log-'+s.name);if(e)e.textContent=j.log}}));
}}
function act(name,op){{fetch(API+'/api/'+op+'?name='+name,{{method:'POST'}}).then(()=>load())}}
function load(){{fetch(API+'/api/services').then(r=>r.json()).then(render)}}
load();setInterval(load,3000);
</script>",
        host
    );
    layout("dashboard", "Dashboard", &body)
}

pub fn download(host: &str) -> String {
    let body = format!(
        "<div class=card><div class=row>
<input id=url placeholder='TikTok or YouTube URL' size=60>
<select id=kind><option value=tt>TikTok</option><option value=yt>YouTube</option></select>
<select id=format><option value=mp4>mp4</option><option value=mp3>mp3</option><option value=best>best</option><option value=audio>audio</option></select>
<button onclick=go()>Download</button></div><p class=muted id=status></p></div>
<script>
const API='http://{}:8080';
async function go(){{
 const url=document.getElementById('url').value.trim();
 const kind=document.getElementById('kind').value;
 const fmt=document.getElementById('format').value;
 const st=document.getElementById('status');
 if(!url){{st.textContent='Enter a URL';return}}
 st.textContent='Fetching...';
 const target=kind==='tt'?`http://{}:8082/download?url=${{encodeURIComponent(url)}}&format=${{fmt}}`:`http://{}:8085/api/download?v=${{encodeURIComponent(url)}}&format=${{fmt}}`;
 try{{
  const r=await fetch(target);
  if(!r.ok){{const j=await r.json();st.textContent=j.error||('HTTP '+r.status);return}}
  const blob=await r.blob();
  const cd=r.headers.get('content-disposition')||'';
  const m=cd.match(/filename=\"([^\"]+)\"/);
  const name=m?m[1]:'download.bin';
  const a=document.createElement('a');a.href=URL.createObjectURL(blob);a.download=name;a.click();
  st.textContent='Saved '+name;
 }}catch(e){{st.textContent='Error: '+e.message}}
}}
</script>",
        host, host, host
    );
    layout("download", "Download", &body)
}

pub fn watch() -> String {
    let body = "<div class=card><div class=row>
<input id=vid placeholder='YouTube video ID or URL' size=50>
<button onclick=play()>Play</button>
<a href='/download'>Download instead</a></div>
<video id=player controls></video><p class=muted id=status></p></div>
<script>
function id(v){
 const m=v.match(/(?:v=|youtu\\.be\\/|shorts\\/)([A-Za-z0-9_-]{11})/);
 return m?m[1]:v.trim();
}
function play(){
 const v=document.getElementById('vid').value.trim();
 if(!v){document.getElementById('status').textContent='Enter a video ID';return}
 const player=document.getElementById('player');
 const src=location.protocol+'//'+location.hostname+':8085/api/stream?v='+encodeURIComponent(id(v))+'&format=best';
 player.src=src;player.play().catch(()=>{});
 document.getElementById('status').textContent='Streaming '+id(v);
}
</script>";
    layout("watch", "Watch", body)
}

pub fn images(host: &str) -> String {
    let body = format!(
        "<div class=card><div class=row><input type=file id=file accept='image/png'><select id=action>
<option value=bg>Remove background</option><option value=wm>Remove watermark</option></select>
<button onclick=run()>Process</button></div>
<div class=row style='margin-top:10px'><label>threshold</label><input id=threshold value='32' size=6>
<label>color (bg)</label><input id=color placeholder='00ff00' size=8></div>
<div class=row style='margin-top:6px'><label>watermark box</label>
<input id=x placeholder=x size=4><input id=y placeholder=y size=4><input id=w placeholder=w size=5><input id=h placeholder=h size=5></div>
<p class=muted id=status></p></div>
<div class=card><img id=result style='display:none'><p><a id=save style='display:none' download='result.png'>Download result</a></p></div>
<script>
async function run(){{
 const f=document.getElementById('file').files[0];
 const st=document.getElementById('status');
 if(!f){{st.textContent='Choose a PNG';return}}
 const action=document.getElementById('action').value;
 let q='threshold='+document.getElementById('threshold').value;
 if(action==='bg'){{
  const c=document.getElementById('color').value.trim();if(c)q+='&color='+c;
 }} else {{
  for(const k of ['x','y','w','h']){{const v=document.getElementById(k).value.trim();if(v)q+='&'+k+'='+v}}
 }}
 const port=action==='bg'?'8084':'8081';
 const path=action==='bg'?'/remove-bg':'/remove-watermark';
 const fd=new FormData();fd.append('image',f);
 st.textContent='Processing...';
 try{{
  const r=await fetch(`http://{}:${{port}}${{path}}?${{q}}`,{{method:'POST',body:fd}});
  if(!r.ok){{const j=await r.json();st.textContent=j.error||('HTTP '+r.status);return}}
  const blob=await r.blob();
  const url=URL.createObjectURL(blob);
  const img=document.getElementById('result');img.src=url;img.style.display='block';
  const a=document.getElementById('save');a.href=url;a.style.display='inline';
  st.textContent='Done';
 }}catch(e){{st.textContent='Error: '+e.message}}
}}
</script>",
        host
    );
    layout("images", "Images", &body)
}

pub fn about() -> String {
    let body = "<div class=card>
<p>Repository: <a href='https://github.com/Kandoraq/local-restapi'>github.com/Kandoraq/local-restapi</a></p>
<p>License: <a href='https://github.com/Kandoraq/local-restapi/blob/main/LICENSE'>MIT</a></p>
<p>Developer: <a href='https://github.com/Kandoraq'>github.com/Kandoraq</a></p>
</div>";
    layout("about", "About", body)
}

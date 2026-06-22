#!/usr/bin/env python3
"""Render `wip --json` into a standalone self-contained HTML dashboard.

Usage:  python3 dashboard.py [extra wip args...]   # e.g. --no-gh
Writes ~/wip-dashboard.html and prints its path. Re-run to refresh.
Data is inlined into the HTML so it opens via file:// with no server/CORS.
ponytail: regenerate-on-demand, no live server. Add `wip serve` only if you
actually want auto-refresh without re-running this.
"""
import json, os, subprocess, sys, html

args = sys.argv[1:] or ["--no-gh"]
raw = subprocess.run(["wip", "--json", *args], capture_output=True, text=True, check=True).stdout
repos = json.loads(raw)

out = os.path.expanduser("~/wip-dashboard.html")
PAGE = """<!doctype html><html lang="zh-Hant"><head><meta charset="utf-8">
<meta name="viewport" content="width=device-width,initial-scale=1">
<title>wip dashboard</title>
<style>
:root{color-scheme:dark}
*{box-sizing:border-box}
body{margin:0;font:15px/1.5 system-ui,-apple-system,"Noto Sans TC",sans-serif;
  background:#0e1116;color:#e6edf3;padding:24px}
h1{font-size:20px;margin:0 0 4px}
.sub{color:#8b949e;font-size:13px;margin-bottom:20px}
#grid{display:grid;gap:14px;grid-template-columns:repeat(auto-fill,minmax(320px,1fr))}
.card{background:#161b22;border:1px solid #30363d;border-left-width:4px;border-radius:8px;padding:14px 16px}
.card.fresh{border-left-color:#3fb950}
.card.warm{border-left-color:#d29922}
.card.cold{border-left-color:#6e7681}
.card.err{border-left-color:#f85149}
.name{font-weight:600;font-size:16px;display:flex;justify-content:space-between;align-items:baseline;gap:8px}
.branch{font:12px ui-monospace,monospace;color:#8b949e;background:#21262d;padding:1px 7px;border-radius:10px;white-space:nowrap}
.when{color:#8b949e;font-size:12px;margin:6px 0 2px}
.msg{font-size:13px;color:#c9d1d9;overflow:hidden;display:-webkit-box;-webkit-line-clamp:2;-webkit-box-orient:vertical}
.sha{font:11px ui-monospace,monospace;color:#6e7681}
.badges{margin-top:8px;display:flex;flex-wrap:wrap;gap:6px}
.b{font-size:11px;padding:1px 8px;border-radius:10px;border:1px solid #30363d;color:#8b949e}
.b.d{color:#d29922;border-color:#5c4813}
.b.u{color:#58a6ff;border-color:#1f3a5f}
.b.pr{color:#3fb950;border-color:#1f4429}
.b.x{color:#f85149;border-color:#5c1a17}
.tail{margin-top:8px;font-size:12px;color:#8b949e;border-top:1px solid #21262d;padding-top:6px;white-space:pre-wrap}
</style></head><body>
<h1>wip dashboard</h1>
<div class="sub">__COUNT__ repos · 產生於 __GEN__ · 重跑 <code>python3 ~/wip/dashboard.py</code> 刷新</div>
<div id="grid"></div>
<script>
const repos = __DATA__;
const grid = document.getElementById('grid');
function age(c){if(c.error)return'err';const t=c.commit_ts;if(!t)return'cold';
  const d=(Date.now()/1000-t)/86400;return d<7?'fresh':d<30?'warm':'cold';}
function esc(s){const e=document.createElement('div');e.textContent=s==null?'':s;return e.innerHTML;}
for(const c of repos){
  const lc=c.last_commit||{};
  const badges=[];
  if(c.dirty_files)badges.push(`<span class="b d">${c.dirty_files} dirty</span>`);
  if(c.unpushed)badges.push(`<span class="b u">${c.unpushed} unpushed</span>`);
  if(c.open_prs&&c.open_prs.length)badges.push(`<span class="b pr">${c.open_prs.length} PR</span>`);
  if(c.open_issues)badges.push(`<span class="b">${c.open_issues} issues</span>`);
  if(c.error)badges.push(`<span class="b x">error</span>`);
  const na=(c.next_actions||[]).length?`<div class="tail">NEXT: ${esc(c.next_actions.join(' · '))}</div>`:'';
  const tail=c.progress_tail?`<div class="tail">${esc(c.progress_tail)}</div>`:'';
  grid.insertAdjacentHTML('beforeend',`<div class="card ${age(c)}">
    <div class="name"><span>${esc(c.name)}</span><span class="branch">${esc(c.branch)}</span></div>
    <div class="when">${esc(lc.rel_time||c.error||'—')}</div>
    <div class="msg">${esc(lc.message||'')}</div>
    <div class="sha">${esc(lc.sha||'')}</div>
    <div class="badges">${badges.join('')}</div>${na}${tail}</div>`);
}
</script></body></html>"""

gen = subprocess.run(["date", "+%Y-%m-%d %H:%M"], capture_output=True, text=True).stdout.strip()
page = (PAGE.replace("__DATA__", json.dumps(repos, ensure_ascii=False))
            .replace("__COUNT__", str(len(repos)))
            .replace("__GEN__", html.escape(gen)))
with open(out, "w") as f:
    f.write(page)
print(out)

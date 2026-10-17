#!/usr/bin/env python3
"""DEUS VULT art gallery + review server (local only).

Serves a live-scanned gallery (always current on reload) with per-cell
review state: verdict (approved / needs-work / rejected) and free-text
notes, persisted to art/gallery-notes.json.

Usage: python3 tools/art_gallery_server.py [port]   (default 8137)
"""
from __future__ import annotations

import html
import json
import sys
import time
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
from pathlib import Path
from urllib.parse import parse_qs, urlparse  # noqa: F401

ROOT = Path(__file__).resolve().parent.parent
NOTES = ROOT / "art" / "gallery-notes.json"
EXTS = {".png", ".gif", ".webp", ".jpg", ".jpeg"}

LANES = [
    ("Adversaries (The Road East)", "art/adversaries-staging"),
    ("Champions & Fiends (roster)", "art/roster-staging"),
    ("Animation staging", "art/anim-staging"),
    ("Shipped assets (crusade-app)", "crusade-app/assets"),
]

CSS = """
body{background:#0B0E1B;color:#E8E3D3;font-family:Georgia,serif;margin:0;padding:1.5rem}
h1{color:#F4D35E;border-bottom:2px solid #F4D35E;padding-bottom:.3rem}
h2{color:#F4D35E;margin-top:2rem;cursor:pointer}
.bar{position:sticky;top:0;background:#0B0E1B;padding:.6rem 0;border-bottom:1px solid #2a3350;z-index:5}
button.f{background:#141928;color:#E8E3D3;border:1px solid #2a3350;border-radius:4px;padding:.3rem .7rem;margin-right:.4rem;cursor:pointer}
button.f.on{background:#F4D35E;color:#0B0E1B}
.grid{display:grid;grid-template-columns:repeat(auto-fill,minmax(140px,1fr));gap:12px}
.card{background:#141928;border:1px solid #2a3350;border-radius:6px;padding:7px;text-align:center;cursor:pointer}
.card img{width:120px;image-rendering:pixelated;background:#0B0E1B;border:1px solid #2a3350}
.card .name{font-size:.75rem;margin-top:.3rem;word-break:break-word}
.card .path{font-size:.55rem;color:#7a8099;word-break:break-all}
.badge{display:inline-block;font-size:.6rem;padding:1px 6px;border-radius:8px;margin-top:4px;margin-left:2px}
.b-approved{background:#2d6a4f}.b-needs-work{background:#A62639}.b-rejected{background:#555}
.b-noted{background:#F4D35E;color:#0B0E1B}
#modal{display:none;position:fixed;inset:0;background:rgba(0,0,0,.85);z-index:10;padding:4vh 4vw;overflow:auto}
#modal .inner{background:#141928;border:1px solid #F4D35E;max-width:920px;margin:auto;padding:1.2rem;border-radius:8px}
#modal img{image-rendering:pixelated;width:512px;max-width:100%;background:#0B0E1B}
#modal textarea{width:100%;height:90px;background:#0B0E1B;color:#E8E3D3;border:1px solid #2a3350}
.verdbtn{margin-right:.5rem;padding:.4rem .9rem;cursor:pointer;border-radius:4px;border:1px solid #2a3350;background:#141928;color:#E8E3D3}
.sel-approved{background:#2d6a4f!important}.sel-needs-work{background:#A62639!important}.sel-rejected{background:#555!important}
.small{color:#7a8099;font-size:.8rem}
"""

JS = """
let NOTES = window.__NOTES__ || {};
let cur = null;
function badgeHTML(p){
  let n = NOTES[p] || {}; let s = '';
  if(n.verdict) s += '<span class="badge b-'+n.verdict+'">'+n.verdict+'</span>';
  if(n.note) s += '<span class="badge b-noted">note</span>';
  return s;
}
function openM(p){
  cur = p;
  document.getElementById('mimg').src = '/' + p;
  document.getElementById('mtitle').textContent = p;
  let n = NOTES[p] || {};
  document.getElementById('mnote').value = n.note || '';
  ['approved','needs-work','rejected'].forEach(v => {
    document.getElementById('v-' + v).className = 'verdbtn' + (n.verdict === v ? ' sel-' + v : '');
  });
  document.getElementById('modal').style.display = 'block';
}
function closeM(){ document.getElementById('modal').style.display = 'none'; }
function setVerd(v){
  let n = NOTES[cur] || {};
  if(v) n.verdict = v; else delete n.verdict;
  NOTES[cur] = n; persist();
}
function saveNote(){
  let n = NOTES[cur] || {};
  let t = document.getElementById('mnote').value;
  if(t) n.note = t; else delete n.note;
  NOTES[cur] = n; persist(); closeM();
}
function persist(){
  let n = NOTES[cur] || {};
  fetch('/api/note', {method:'POST', body: JSON.stringify({path: cur, verdict: n.verdict||'', note: n.note||''})})
    .then(() => {
      let host = document.getElementById('card-' + btoa(cur));
      if(host){
        host.setAttribute('data-verd', n.verdict || '');
        let old = host.querySelector('.badges'); if(old) old.remove();
        host.insertAdjacentHTML('beforeend', '<span class="badges">' + badgeHTML(cur) + '</span>');
      }
      if(document.getElementById('modal').style.display === 'block') openM(cur);
      counts();
    });
}
function filt(v, btn){
  document.querySelectorAll('button.f').forEach(b => b.className = 'f');
  btn.className = 'f on';
  document.querySelectorAll('.card').forEach(c => {
    let vd = c.getAttribute('data-verd');
    c.style.display = (v === 'all') || (v === 'unreviewed' && !vd) || (v === vd) ? '' : 'none';
  });
}
function counts(){
  let t = 0, a = 0, n = 0, r = 0;
  document.querySelectorAll('.card').forEach(() => t++);
  Object.values(NOTES).forEach(x => {
    if(x.verdict === 'approved') a++;
    else if(x.verdict === 'needs-work') n++;
    else if(x.verdict === 'rejected') r++;
  });
  document.getElementById('counts').textContent =
    t + ' cells | ' + a + ' approved | ' + n + ' needs work | ' + r + ' rejected';
}
function toggle(el){ el.nextElementSibling.classList.toggle('open'); }
counts();
"""


def load_notes() -> dict:
    if NOTES.exists():
        return json.loads(NOTES.read_text())
    return {}


def save_notes(n: dict) -> None:
    NOTES.write_text(json.dumps(n, indent=1))


def badge_html(n) -> str:
    if not n:
        return ""
    s = ""
    if n.get("verdict"):
        s += f"<span class='badge b-{html.escape(n['verdict'])}'>{html.escape(n['verdict'])}</span>"
    if n.get("note"):
        s += "<span class='badge b-noted'>note</span>"
    return s


def render() -> bytes:
    notes = load_notes()
    chunks = [
        "<!doctype html><html><head><meta charset='utf-8'>",
        "<title>DEUS VULT — Art Review</title>",
        "<style>", CSS, "</style></head><body>",
        "<h1>DEUS VULT — Art Review</h1>",
        "<div class='bar'>",
        "Filter: <button class='f on' onclick=filt('all',this)>all</button>",
        "<button class='f' onclick=filt('unreviewed',this)>unreviewed</button>",
        "<button class='f' onclick=filt('needs-work',this)>needs work</button>",
        "<button class='f' onclick=filt('approved',this)>approved</button>",
        "<button class='f' onclick=filt('rejected',this)>rejected</button>",
        " <span class='small' id='counts'></span></div>",
    ]
    for heading, root in LANES:
        base = ROOT / root
        files = []
        if base.exists():
            files = sorted(
                str(p.relative_to(ROOT)) for p in base.rglob("*") if p.suffix.lower() in EXTS
            )
        if not files:
            chunks.append(f"<h2>{html.escape(heading)}</h2><p class='small'>— empty —</p>")
            continue
        chunks.append(
            f"<h2 onclick='toggle(this)'>{html.escape(heading)} ({len(files)})</h2>"
            "<div class='section'><div class='grid'>"
        )
        for p in files:
            esc = html.escape(p)
            esc_js = p.replace("\\", "\\\\").replace("'", "\\'")
            verd = html.escape((notes.get(p) or {}).get("verdict", ""))
            chunks.append(
                f"<div class='card' id='card-__B64__{esc}' data-verd='{verd}' onclick=\"openM('{esc_js}')\">"
                f"<img loading='lazy' src='/{esc}'>"
                f"<div class='name'>{html.escape(Path(p).stem)}</div>"
                f"<div class='path'>{esc}</div>"
                f"<span class='badges'>{badge_html(notes.get(p))}</span></div>"
            )
        chunks.append("</div></div>")
    chunks += [
        "<div id='modal'><div class='inner'>",
        "<img id='mimg'><div id='mtitle' style='margin:.5rem 0'></div>",
        "<div style='margin:.6rem 0'>",
        "<button class='verdbtn' id='v-approved' onclick=setVerd('approved')>Approved</button>",
        "<button class='verdbtn' id='v-needs-work' onclick=setVerd('needs-work')>Needs work</button>",
        "<button class='verdbtn' id='v-rejected' onclick=setVerd('rejected')>Rejected</button>",
        "<button class='verdbtn' onclick=setVerd('')>Clear verdict</button></div>",
        "<textarea id='mnote' placeholder='tweak notes (e.g. cross too small, recolor banner)'></textarea>",
        "<div style='margin-top:.5rem'><button class='verdbtn' onclick='saveNote()'>Save note</button>",
        "<button class='verdbtn' onclick='closeM()'>Close</button></div>",
        "</div></div>",
        "<script>window.__NOTES__ = ", json.dumps(notes), ";", JS, "</script></body></html>",
    ]
    return "\n".join(chunks).encode()


class H(BaseHTTPRequestHandler):
    def log_message(self, *a):
        pass

    def _send(self, body: bytes, ctype: str, code: int = 200):
        self.send_response(code)
        self.send_header("Content-Type", ctype)
        self.send_header("Content-Length", str(len(body)))
        self.end_headers()
        self.wfile.write(body)

    def do_GET(self):
        u = urlparse(self.path).path
        if u in ("/", "/gallery", "/art/gallery.html"):
            return self._send(render(), "text/html")
        p = (ROOT / u.lstrip("/")).resolve()
        if p.is_file() and str(p).startswith(str(ROOT)) and p.suffix.lower() in EXTS:
            mime = {
                ".png": "image/png",
                ".gif": "image/gif",
                ".jpg": "image/jpeg",
                ".jpeg": "image/jpeg",
                ".webp": "image/webp",
            }[p.suffix.lower()]
            return self._send(p.read_bytes(), mime)
        self._send(b"404", "text/plain", 404)

    def do_POST(self):
        if urlparse(self.path).path == "/api/note":
            n = int(self.headers.get("Content-Length", 0))
            d = json.loads(self.rfile.read(n) or b"{}")
            path = d.get("path", "")
            if not path.startswith(("art/", "crusade-app/assets/")):
                return self._send(b"bad path", "text/plain", 400)
            notes = load_notes()
            entry = notes.get(path, {})
            if d.get("verdict"):
                entry["verdict"] = d["verdict"]
            else:
                entry.pop("verdict", None)
            if d.get("note"):
                entry["note"] = d["note"]
            else:
                entry.pop("note", None)
            if entry:
                entry["ts"] = int(time.time())
                notes[path] = entry
            else:
                notes.pop(path, None)
            save_notes(notes)
            return self._send(b"ok", "text/plain")
        self._send(b"404", "text/plain", 404)


if __name__ == "__main__":
    port = int(sys.argv[1]) if len(sys.argv) > 1 else 8137
    print(f"art review server -> http://localhost:{port}/")
    ThreadingHTTPServer(("127.0.0.1", port), H).serve_forever()

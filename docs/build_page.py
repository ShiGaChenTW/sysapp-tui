#!/usr/bin/env python3
"""Generate docs/index.html.

The terminal frames are real captures from the shipped binary, converted to
styled HTML by capture.py — no mockups, no screenshots-as-images. That also
means the page needs no image assets at all.
"""
import json, pathlib

SHOTS = json.load(open("shots.json"))
OUT = pathlib.Path("/Users/scottchen/Documents/20_Projects/Project_sysapp-tui/docs/index.html")

TABS = [
    ("browse", "BROWSE", "906 units, sorted by real invocation count. The meter column is the only place colour carries data."),
    ("idle",   "IDLE",   "Only units with no evidence of use — zero shell invocations and no recent open."),
    ("detail", "RECORD", "One unit in full. Overlays are focus traps; the grid keeps its position underneath."),
    ("help",   "KEYS",   "Tier two of the help system. The footer carries five keys, this carries the rest."),
]

def tabs_markup():
    inputs, labels, panes = [], [], []
    for i, (key, label, caption) in enumerate(TABS):
        checked = " checked" if i == 0 else ""
        inputs.append(f'<input type="radio" name="shot" id="tab-{key}" class="tabin"{checked}>')
        labels.append(f'<label for="tab-{key}" class="tab">{label}</label>')
        panes.append(
            f'<div class="pane" id="pane-{key}">'
            f'<div class="termwrap"><pre class="term">{SHOTS[key]}</pre></div>'
            f'<p class="cap"><span class="mark">///</span> {caption}</p>'
            f"</div>"
        )
    return "\n".join(inputs), "\n".join(labels), "\n".join(panes)


INPUTS, LABELS, PANES = tabs_markup()

# Sibling selectors bind each radio to its pane — CSS-only tabs, so the page
# works with JavaScript disabled.
TAB_RULES = "\n".join(
    f"#tab-{k}:checked ~ .tabbar label[for=tab-{k}]{{background:var(--accent);color:var(--bg);}}\n"
    f"#tab-{k}:checked ~ .panes #pane-{k}{{display:block;}}"
    for k, _, _ in TABS
)

SOURCES = [
    ("HOMEBREW", "brew info --json=v2", "322"),
    ("CASK", "brew info --json=v2", "58"),
    ("APPLICATIONS", "system_profiler", "287"),
    ("CARGO", "~/.cargo/bin", "47"),
    ("GO", "~/go/bin", "6"),
    ("NPM", "npm list -g", "2"),
    ("PIP", "pip3 list", "32"),
    ("GEM", "gem list", "48"),
    ("PKGUTIL", "pkgutil --pkgs", "117"),
]

src_rows = "\n".join(
    f'<div class="srow"><b>{n}</b><samp>{c}</samp><data>{v}</data></div>'
    for n, c, v in SOURCES
)

HTML = f"""<!doctype html>
<html lang="en">
<head>
<meta charset="utf-8">
<meta name="viewport" content="width=device-width,initial-scale=1">
<title>sysapp-tui — macOS system package scanner</title>
<meta name="description" content="Scans nine local package sources in one pass, detects language, analyses real usage. Opens in 10ms. Rust + ratatui on The Elm Architecture.">
<meta name="color-scheme" content="dark">
<meta property="og:title" content="sysapp-tui">
<meta property="og:description" content="macOS system package scanner and TUI dashboard. 906 packages, nine sources, opens in 10ms.">
<meta property="og:type" content="website">
<link rel="icon" href="data:image/svg+xml,<svg xmlns='http://www.w3.org/2000/svg' viewBox='0 0 16 16'><rect width='16' height='16' fill='%230A0A0A'/><rect x='2' y='3' width='12' height='2' fill='%23E61919'/><rect x='2' y='7' width='8' height='2' fill='%23EAEAEA'/><rect x='2' y='11' width='10' height='2' fill='%23EAEAEA'/></svg>">
<style>
/* ── Substrate ──────────────────────────────────────────────────────────
   Tactical Telemetry only. The palette is lifted verbatim from
   src/tui/theme.rs, so the page and the product are one visual system.
   Terminal green appears nowhere except inside the captured frames, where
   it carries the usage meter — the same single-element rule the TUI keeps. */
:root{{
  --bg:#0A0A0A; --surface:#121212; --overlay:#1A1A1A;
  --fg:#EAEAEA; --muted:#6E6E6E; --rule:#2A2A2A;
  --accent:#E61919;
  --mono:ui-monospace,SFMono-Regular,"SF Mono",Menlo,Consolas,"Liberation Mono",monospace;
  --sans:-apple-system,BlinkMacSystemFont,"Inter","Helvetica Neue",Arial,sans-serif;
}}
*,*::before,*::after{{box-sizing:border-box;border-radius:0!important}}
html{{-webkit-text-size-adjust:100%}}
body{{
  margin:0;background:var(--bg);color:var(--fg);
  font-family:var(--mono);font-size:13px;line-height:1.5;
  -webkit-font-smoothing:antialiased;
  overflow-x:hidden;
}}
/* Mechanical grain + CRT sweep. Pinned, non-interactive, and cheap: two
   gradients rather than a bitmap, so there is nothing to download. */
body::after{{
  content:"";position:fixed;inset:0;pointer-events:none;z-index:99;
  background:
    repeating-linear-gradient(0deg,transparent,transparent 2px,rgba(0,0,0,.16) 2px,rgba(0,0,0,.16) 3px);
  opacity:.5;
}}
@media (prefers-reduced-motion:reduce){{body::after{{display:none}}}}

.wrap{{max-width:1180px;margin:0 auto;padding-inline:20px}}
hr.rule{{border:0;border-top:1px solid var(--rule);margin:0}}
.mark{{color:var(--accent)}}
a{{color:var(--fg);text-decoration:none;border-bottom:1px solid var(--accent)}}
a:hover{{background:var(--accent);color:var(--bg);border-color:var(--accent)}}

/* ── Masthead ─────────────────────────────────────────────────────────── */
.bar{{
  position:sticky;top:0;z-index:50;background:var(--bg);
  border-bottom:1px solid var(--rule);
}}
.barin{{display:flex;align-items:center;gap:14px;height:38px;
  font-size:11px;letter-spacing:.1em;text-transform:uppercase}}
.barin .sp{{flex:1}}
.barin b{{font-weight:700;letter-spacing:.05em}}
.barin .m{{color:var(--muted)}}
@media(max-width:620px){{.barin .hide{{display:none}}}}

/* ── Hero ─────────────────────────────────────────────────────────────── */
.hero{{padding-block:clamp(48px,9vw,110px) clamp(36px,6vw,64px)}}
.kicker{{font-size:11px;letter-spacing:.18em;color:var(--accent);
  text-transform:uppercase;margin:0 0 22px}}
h1{{
  font-family:var(--sans);font-weight:900;text-transform:uppercase;
  font-size:clamp(2.9rem,11.5vw,9.5rem);line-height:.86;letter-spacing:-.045em;
  margin:0;
}}
h1 .thin{{color:var(--accent)}}
.lede{{
  max-width:66ch;margin:30px 0 0;font-size:15px;line-height:1.65;color:var(--fg);
}}
.lede .m{{color:var(--muted)}}

/* ── Metric strip ─────────────────────────────────────────────────────── */
.metrics{{
  display:grid;grid-template-columns:repeat(auto-fit,minmax(168px,1fr));
  gap:1px;background:var(--rule);border-top:1px solid var(--rule);
  border-bottom:1px solid var(--rule);
}}
.metricwrap{{max-width:1180px;margin:0 auto;padding-inline:20px}}
.metric{{background:var(--bg);padding:24px 18px}}
.metric .lab{{font-size:10px;letter-spacing:.15em;color:var(--muted);
  text-transform:uppercase}}
.metric .val{{font-family:var(--sans);font-weight:900;letter-spacing:-.04em;
  font-size:clamp(2rem,5.2vw,3.4rem);line-height:1;margin-top:10px}}
.metric .val.hot{{color:var(--accent)}}
.metric .sub{{font-size:11px;color:var(--muted);margin-top:8px;letter-spacing:.04em}}
.metric del{{color:var(--muted);text-decoration-color:var(--accent);
  text-decoration-thickness:2px}}

/* ── Section scaffolding ──────────────────────────────────────────────── */
section{{padding-block:clamp(46px,7vw,84px)}}
.shead{{display:flex;align-items:baseline;gap:14px;margin:0 0 28px;flex-wrap:wrap}}
.shead h2{{
  font-family:var(--sans);font-weight:900;text-transform:uppercase;
  font-size:clamp(1.5rem,3.6vw,2.5rem);letter-spacing:-.03em;line-height:1;margin:0;
}}
.shead .n{{font-size:11px;letter-spacing:.15em;color:var(--accent)}}
.shead p{{margin:0;color:var(--muted);font-size:12px;letter-spacing:.04em;flex-basis:100%}}

/* ── Terminal frames ──────────────────────────────────────────────────── */
.tabin{{position:absolute;opacity:0;pointer-events:none}}
.tabbar{{display:flex;gap:1px;background:var(--rule);border:1px solid var(--rule);
  border-bottom:0;flex-wrap:wrap}}
.tab{{
  background:var(--bg);color:var(--muted);cursor:pointer;
  padding:10px 20px;font-size:11px;letter-spacing:.13em;text-transform:uppercase;
  user-select:none;
}}
.tab:hover{{color:var(--fg)}}
.tabin:focus-visible ~ .tabbar label{{outline:2px solid var(--accent);outline-offset:-2px}}
.panes{{border:1px solid var(--rule)}}
.pane{{display:none}}
{TAB_RULES}
/* The frame is 118 columns; on a narrow viewport it scrolls inside its own
   box rather than forcing the page sideways. */
.termwrap{{overflow-x:auto;background:var(--bg);padding:16px 0}}
pre.term{{
  margin:0;padding:0 18px;font-family:var(--mono);
  font-size:11.5px;line-height:1.34;white-space:pre;
  display:inline-block;min-width:100%;
}}
@media(max-width:900px){{pre.term{{font-size:9.6px;line-height:1.3}}}}
@media(max-width:520px){{pre.term{{font-size:7.6px}}}}
.cap{{margin:0;padding:13px 18px;border-top:1px solid var(--rule);
  color:var(--muted);font-size:11.5px;letter-spacing:.03em;background:var(--surface)}}

/* ── Source grid ──────────────────────────────────────────────────────── */
.sources{{display:grid;gap:1px;background:var(--rule);border:1px solid var(--rule)}}
.srow{{
  background:var(--bg);display:grid;
  grid-template-columns:minmax(120px,1.1fr) minmax(0,2fr) 74px;
  gap:12px;padding:12px 16px;align-items:baseline;
}}
.srow b{{font-weight:700;letter-spacing:.09em;font-size:12px}}
.srow samp{{color:var(--muted);font-size:11.5px;overflow-wrap:anywhere}}
.srow data{{text-align:right;font-family:var(--sans);font-weight:900;font-size:16px;
  letter-spacing:-.02em}}
@media(max-width:560px){{
  .srow{{grid-template-columns:1fr 62px}}
  .srow samp{{grid-column:1/-1;order:3}}
}}

/* ── Install ──────────────────────────────────────────────────────────── */
.install{{border:2px solid var(--accent);background:var(--surface)}}
.install .ihead{{background:var(--accent);color:var(--bg);padding:8px 16px;
  font-size:11px;letter-spacing:.15em;font-weight:700;text-transform:uppercase}}
.install pre{{margin:0;padding:20px 16px;overflow-x:auto;font-size:14px}}
.install .c{{color:var(--muted)}}

/* ── Notes (two-up prose blocks) ──────────────────────────────────────── */
.notes{{display:grid;gap:1px;background:var(--rule);border:1px solid var(--rule);
  grid-template-columns:repeat(auto-fit,minmax(310px,1fr))}}
.note{{background:var(--bg);padding:22px 20px}}
.note h3{{margin:0 0 12px;font-size:12px;letter-spacing:.13em;text-transform:uppercase;
  color:var(--accent);font-weight:700}}
.note p{{margin:0 0 11px;font-size:12.5px;line-height:1.62;color:var(--fg)}}
.note p:last-child{{margin-bottom:0}}
.note .m{{color:var(--muted)}}
.note code{{background:var(--overlay);padding:1px 5px;font-size:11.5px}}

/* ── Caveat ───────────────────────────────────────────────────────────── */
.caveat{{border:1px solid var(--accent);padding:20px;background:var(--surface)}}
.caveat h3{{margin:0 0 10px;font-size:12px;letter-spacing:.13em;color:var(--accent);
  text-transform:uppercase}}
.caveat p{{margin:0 0 10px;font-size:12.5px;line-height:1.62}}
.caveat p:last-child{{margin:0;color:var(--muted)}}
.stripe{{height:14px;margin-bottom:20px;
  background:repeating-linear-gradient(45deg,var(--accent) 0 12px,var(--bg) 12px 24px)}}

/* ── Keys ─────────────────────────────────────────────────────────────── */
.keys{{display:grid;gap:1px;background:var(--rule);border:1px solid var(--rule);
  grid-template-columns:repeat(auto-fit,minmax(228px,1fr))}}
.key{{background:var(--bg);padding:13px 16px;display:flex;gap:12px;align-items:baseline}}
.key kbd{{
  font-family:var(--mono);background:var(--overlay);border:1px solid var(--rule);
  padding:2px 7px;font-size:11px;color:var(--accent);white-space:nowrap;flex:none;
}}
.key span{{font-size:12px;color:var(--muted)}}

/* ── Footer ───────────────────────────────────────────────────────────── */
footer{{border-top:1px solid var(--rule);padding:26px 0 46px;color:var(--muted);
  font-size:11px;letter-spacing:.08em}}
footer .frow{{display:flex;gap:18px;flex-wrap:wrap;align-items:center}}
footer .sp{{flex:1}}
</style>
</head>
<body>

<div class="bar"><div class="wrap barin">
  <b>SYSAPP<span class="mark">·</span>TUI</b>
  <span class="mark">®</span>
  <span class="m hide">REV 0.2.0 / UNIT D-01</span>
  <span class="sp"></span>
  <a href="https://github.com/ShiGaChenTW/sysapp-tui">SOURCE</a>
  <a href="https://github.com/ShiGaChenTW/sysapp-tui/releases/tag/v0.2.0" class="hide">RELEASE</a>
</div></div>

<header class="wrap hero">
  <p class="kicker">[ MACOS PACKAGE INVENTORY ] &nbsp; NINE SOURCES / ONE PASS / ZERO NETWORK</p>
  <h1>Every<br>package<br><span class="thin">you own.</span></h1>
  <p class="lede">
    <code>sysapp-tui</code> scans Homebrew, Cask, Applications, Cargo, Go, npm, pip, gem and
    pkgutil in a single pass, then tells you what each one is written in and whether you have
    ever actually used it.
    <span class="m">Completely offline — every field comes from a local system command.</span>
  </p>
</header>

<div class="metricwrap"><div class="metrics">
  <div class="metric">
    <div class="lab">Launch</div>
    <div class="val hot">10<span style="font-size:.4em">MS</span></div>
    <div class="sub">was <del>88.9s</del> — cached snapshot</div>
  </div>
  <div class="metric">
    <div class="lab">Inventory</div>
    <div class="val">906</div>
    <div class="sub">packages on the test machine</div>
  </div>
  <div class="metric">
    <div class="lab">Sources</div>
    <div class="val">9</div>
    <div class="sub">queried concurrently</div>
  </div>
  <div class="metric">
    <div class="lab">Network</div>
    <div class="val">0</div>
    <div class="sub">requests, by design</div>
  </div>
</div></div>

<section class="wrap">
  <div class="shead">
    <span class="n">01</span><h2>The interface</h2>
    <p>Real frames from the shipped binary — captured through a pty, not redrawn.</p>
  </div>
  {INPUTS}
  <div class="tabbar">{LABELS}</div>
  <div class="panes">{PANES}</div>
</section>

<hr class="rule">

<section class="wrap">
  <div class="shead">
    <span class="n">02</span><h2>Install</h2>
    <p>macOS 12+ · Apple Silicon and Intel</p>
  </div>
  <div class="install">
    <div class="ihead">[ HOMEBREW ]</div>
<pre>brew install ShiGaChenTW/tap/sysapp-tui</pre>
  </div>
  <div class="install" style="margin-top:18px;border-color:var(--rule)">
    <div class="ihead" style="background:var(--rule);color:var(--fg)">[ FROM SOURCE ]</div>
<pre><span class="c"># requires the Rust toolchain</span>
git clone https://github.com/ShiGaChenTW/sysapp-tui.git
cd sysapp-tui &amp;&amp; cargo build --release</pre>
  </div>
</section>

<hr class="rule">

<section class="wrap">
  <div class="shead">
    <span class="n">03</span><h2>Sources</h2>
    <p>Counts are from one real run. Same-name packages are merged, richest source wins.</p>
  </div>
  <div class="sources">{src_rows}</div>
</section>

<hr class="rule">

<section class="wrap">
  <div class="shead">
    <span class="n">04</span><h2>Keys</h2>
    <p>Keyboard only. Three tiers: footer, <kbd>?</kbd> overlay, then the README.</p>
  </div>
  <div class="keys">
    <div class="key"><kbd>j / k</kbd><span>move</span></div>
    <div class="key"><kbd>g / G</kbd><span>first / last</span></div>
    <div class="key"><kbd>1 … 7</kbd><span>sort column, repeat to reverse</span></div>
    <div class="key"><kbd>/</kbd><span>live filter</span></div>
    <div class="key"><kbd>Enter</kbd><span>open unit record</span></div>
    <div class="key"><kbd>p</kbd><span>show / hide packaging noise</span></div>
    <div class="key"><kbd>s</kbd><span>only units with no evidence of use</span></div>
    <div class="key"><kbd>r</kbd><span>rescan in background</span></div>
    <div class="key"><kbd>?</kbd><span>key reference</span></div>
    <div class="key"><kbd>q</kbd><span>quit</span></div>
  </div>
</section>

<hr class="rule">

<section class="wrap">
  <div class="shead">
    <span class="n">05</span><h2>Engineering notes</h2>
    <p>The decisions that shaped it, including the ones that were wrong first.</p>
  </div>
  <div class="notes">
    <div class="note">
      <h3>[ 89 seconds → 10 milliseconds ]</h3>
      <p>A cold scan costs about 89 seconds, and <code>brew info --json=v2 --installed</code>
      is 38 of them on its own.</p>
      <p class="m">That cost cannot be optimised away — the scanners already run concurrently
      and brew is queried exactly once. So the fix was not to scan at launch. The inventory is
      cached, schema-versioned, and written temp-file-then-rename so an interrupted write cannot
      leave a truncated snapshot. Press <code>r</code> to rescan in the background; the interface
      stays fully responsive while it runs.</p>
    </div>
    <div class="note">
      <h3>[ Why not Hojicha ]</h3>
      <p>The TUI runs The Elm Architecture on the <code>tears</code> runtime. Hojicha — the
      Bubble Tea port — was evaluated and could not be used.</p>
      <p class="m">Its 0.2.1 line pins ratatui 0.29 while <code>tears</code> needs 0.30, which
      puts two semver-incompatible ratatui crates in one binary: a Hojicha widget cannot be
      rendered into a <code>tears</code> frame. Its 0.2.2 line drops ratatui entirely for
      <code>Model::view(&amp;self) -&gt; String</code>. Verified with <code>cargo tree</code>
      before committing to either.</p>
    </div>
    <div class="note">
      <h3>[ Found by running it, not reading it ]</h3>
      <p>Four defects survived code review and died to a real terminal:</p>
      <p class="m">the usage column sorted least-used-first while displaying <code>▲</code>;
      sorting stranded the viewport mid-list; truncation counted characters rather than display
      columns, so CJK names overflowed; and moving the scan behind the TUI let stray
      <code>eprintln!</code> output paint over live frames. Every fix carries a regression test.</p>
    </div>
    <div class="note">
      <h3>[ 54 tests, no terminal required ]</h3>
      <p><code>update</code> is a pure function of <code>(state, Message)</code> and touches no
      terminal. <code>view</code> is a pure function of state and mutates nothing.</p>
      <p class="m">So input handling and state transitions are testable without a tty, and layout
      is covered by render tests that draw real frames through ratatui's <code>TestBackend</code>.
      <code>cargo test render -- --nocapture</code> prints every frame.</p>
    </div>
  </div>
</section>

<hr class="rule">

<section class="wrap">
  <div class="stripe"></div>
  <div class="caveat">
    <h3>[ Known limitation ]</h3>
    <p>Applications installed under <code>/Applications</code> are currently missing from the
    inventory. <code>system_profiler</code> only reports bundles under <code>/System/</code>
    unless the process has Full Disk Access, so on a typical machine roughly 146 user-installed
    apps do not appear.</p>
    <p>Tracked for the next release; the fix is to enumerate the bundles directly rather than
    ask for a broad permission.</p>
  </div>
</section>

<footer><div class="wrap frow">
  <span>SYSAPP<span class="mark">·</span>TUI <span class="mark">®</span> REV 0.2.0</span>
  <span>MIT</span>
  <span class="sp"></span>
  <a href="https://github.com/ShiGaChenTW/sysapp-tui">GITHUB</a>
  <a href="https://github.com/ShiGaChenTW/sysapp-tui/blob/main/README-ZH.md">中文</a>
</div></footer>

</body>
</html>
"""

OUT.write_text(HTML)
print(f"wrote {OUT} — {len(HTML)//1024} KB")

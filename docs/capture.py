"""Drive the real binary in a pty and emit each frame as color-preserving HTML.

The landing page shows actual product output, not a hand-drawn mockup, so the
capture has to keep the SGR attributes rather than stripping them.
"""
import os, pty, time, select, fcntl, termios, struct, re, sys, json, html

# The local release build, not the installed one: the page has to show the
# version being shipped, and `brew install` lags a release by a tap update.
BIN = '/Users/scottchen/Documents/20_Projects/Project_sysapp-tui/target/release/sysapp-tui'
# Wide enough for the full nine-column tier (needs ~96 columns of grid) plus
# the 36-column record panel. Below this the grid drops columns by design.
ROWS, COLS = 30, 150

def run(keys, stop_marker, settle=0.8, timeout=120, cols=None):
    pid, fd = pty.fork()
    if pid == 0:
        os.environ['TERM'] = 'xterm-256color'
        os.environ['COLORTERM'] = 'truecolor'
        os.environ['LANG'] = 'en_US.UTF-8'
        os.execv(BIN, ['sysapp-tui'])
    width = cols or COLS
    fcntl.ioctl(fd, termios.TIOCSWINSZ, struct.pack('HHHH', ROWS, width, 0, 0))
    t0 = time.time(); buf = b''; grabbed = None; ki = 0; last = 0
    booted = False; stable_at = None
    while time.time() - t0 < timeout:
        r, _, _ = select.select([fd], [], [], 0.05)
        if r:
            try: c = os.read(fd, 65536)
            except OSError: break
            if not c: break
            buf += c
        plain = re.sub(r'\x1b\[[0-9;?]*[a-zA-Z]', '', buf[-60000:].decode('utf8', 'replace'))
        if not booted and 'SYSAPP' in plain:
            booted = True; last = time.time()
        if booted and ki < len(keys) and time.time() - last > 0.7:
            os.write(fd, keys[ki].encode()); ki += 1; last = time.time()
        if stable_at is None and ki >= len(keys) and booted and stop_marker(plain):
            stable_at = time.time()
        if stable_at is not None and time.time() - stable_at > settle:
            grabbed = buf[:]
            break
    os.close(fd)
    try: os.waitpid(pid, 0)
    except OSError: pass
    return grabbed or buf

# ---- ANSI → styled cell grid ----------------------------------------------
def to_grid(data, cols=None):
    txt = data.decode('utf8', 'replace')
    txt = txt.split('\x1b[?1049h', 1)[-1].split('\x1b[?1049l', 1)[0]
    width = cols or COLS
    blank = lambda: [{'ch': ' ', 'fg': None, 'bg': None, 'b': False, 'd': False} for _ in range(width)]
    grid = [blank() for _ in range(ROWS)]
    r = c = 0
    cur = {'fg': None, 'bg': None, 'b': False, 'd': False}
    tok = re.compile(r'\x1b\[([0-9;:?]*)([a-zA-Z])|\x1b\][^\x07\x1b]*(?:\x07|\x1b\\)|\x1b.')
    i = 0
    while i < len(txt):
        m = tok.match(txt, i)
        if m:
            params, cmd = m.group(1), m.group(2)
            if cmd == 'H':
                p = (params or '').split(';')
                r = int(p[0]) - 1 if p and p[0] else 0
                c = int(p[1]) - 1 if len(p) > 1 and p[1] else 0
            elif cmd == 'J' and params in ('2', ''):
                grid = [blank() for _ in range(ROWS)]
            elif cmd == 'm':
                ps = [x for x in (params or '0').replace(':', ';').split(';')]
                k = 0
                while k < len(ps):
                    v = ps[k] or '0'
                    if v == '0': cur = {'fg': None, 'bg': None, 'b': False, 'd': False}
                    elif v == '1': cur['b'] = True
                    elif v == '2': cur['d'] = True
                    elif v == '22': cur['b'] = cur['d'] = False
                    elif v == '39': cur['fg'] = None
                    elif v == '49': cur['bg'] = None
                    elif v in ('38', '48') and k + 4 < len(ps) and ps[k+1] == '2':
                        key = 'fg' if v == '38' else 'bg'
                        cur[key] = f"#{int(ps[k+2]):02x}{int(ps[k+3]):02x}{int(ps[k+4]):02x}"
                        k += 4
                    k += 1
            i = m.end(); continue
        ch = txt[i]; i += 1
        if ch == '\n': r += 1; c = 0
        elif ch == '\r': c = 0
        elif ch >= ' ':
            if 0 <= r < ROWS and 0 <= c < width:
                grid[r][c] = {'ch': ch, **cur}
            c += 1
    return grid

def to_html(grid):
    """Collapse runs of identical style into spans — one span per glyph would
    quadruple the page size for no visual gain."""
    out = []
    for row in grid:
        # trim trailing blanks so the markup does not carry dead columns
        end = len(row)
        while end > 0 and row[end-1]['ch'] == ' ' and not row[end-1]['bg']:
            end -= 1
        line = []
        run_txt, run_key = '', None
        def flush():
            if not run_txt: return
            st = []
            fg, bg, b, d = run_key
            if fg: st.append(f'color:{fg}')
            if bg: st.append(f'background:{bg}')
            if b: st.append('font-weight:700')
            if d: st.append('opacity:.55')
            esc = html.escape(run_txt).replace(' ', '&nbsp;')
            line.append(f'<span style="{";".join(st)}">{esc}</span>' if st else esc)
        for cell in row[:end]:
            key = (cell['fg'], cell['bg'], cell['b'], cell['d'])
            if key != run_key:
                flush(); run_txt, run_key = cell['ch'], key
            else:
                run_txt += cell['ch']
        flush()
        out.append(''.join(line) or '&nbsp;')
    while out and out[-1] == '&nbsp;': out.pop()
    return '\n'.join(out)

SHOTS = {}

# 1. browse (default view, system items hidden)
# Usage-sorted: the meters are the point of the column.
SHOTS['browse'] = run(['6'], lambda p: 'USAGE ▼' in p)
# 2. idle-only view
SHOTS['idle'] = run(['s'], lambda p: 'IDLE ONLY' in p)
# 3. detail record
# `i`, never Enter: as of v0.3 Enter arms a launch and opens a confirmation.
# Driving the capture with Enter would put the page's own screenshot script one
# keypress away from running whatever happened to be selected.
# Captured narrow on purpose. Past 116 columns the record is a permanent side
# panel, so `i` is inert and the frame would be byte-identical to `browse`;
# at 100 it is the modal, which also shows the grid dropping to its compact
# column set rather than clipping headers.
SHOTS['detail'] = run(['6', 'i'], lambda p: 'INVOCATIONS' in p, settle=1.2, cols=100)
DETAIL_COLS = 100
# 4. category filter — one press of `c` narrows to the first populated category
SHOTS['category'] = run(['c'], lambda p: 'Development' in p, settle=1.2)
# 5. help overlay
SHOTS['help'] = run(['?'], lambda p: 'q / Ctrl-C' in p, settle=1.2)

result = {k: to_html(to_grid(v, DETAIL_COLS if k == 'detail' else None))
          for k, v in SHOTS.items()}
json.dump(result, open('shots.json', 'w'))
for k, v in result.items():
    print(f"  {k:<8} {len(v):6} bytes html, {v.count(chr(10))+1} lines")

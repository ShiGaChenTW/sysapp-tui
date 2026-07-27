# Product page

`index.html` is generated, not hand-edited.

The terminal frames on the page are real captures from the shipped binary —
`capture.py` drives it in a pty and converts the ANSI output to styled HTML,
preserving the actual colours. No mockups, no image assets.

```bash
# regenerate after a release, from this directory
python3 capture.py      # → shots.json  (needs sysapp-tui on PATH)
python3 build_page.py   # → index.html
```

`build_page.py` writes to an absolute path; adjust `OUT` if the repo moves.

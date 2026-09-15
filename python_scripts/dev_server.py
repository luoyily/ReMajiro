"""Development server for the web shell (web-shell-plan §4).

Static files only — game resources are never served; they reach the engine
through Chrome's folder-picker API. Adds the COOP/COEP headers required for
SharedArrayBuffer (the worker IO bridge).

Usage:
    pip install fastapi uvicorn
    python scripts/dev_server.py [--port 8000] [--web-dir web]

    GET /        → web/index.html (+ main.js / worker.js)
    GET /pkg/*   → wasm-bindgen output (web/pkg), application/wasm MIME
"""

import argparse
from pathlib import Path

from fastapi import FastAPI, HTTPException
from fastapi.responses import FileResponse, Response
import uvicorn

ROOT = Path(__file__).resolve().parent.parent
WEB = ROOT / "web"

app = FastAPI()
@app.middleware("http")
async def coop_coep(request, call_next):
    response = await call_next(request)
    # SAB (worker IO bridge) requires the page to be crossOriginIsolated.
    response.headers["Cross-Origin-Opener-Policy"] = "same-origin"
    response.headers["Cross-Origin-Resource-Policy"] = "same-origin"
    response.headers["Cross-Origin-Embedder-Policy"] = "require-corp"
    # Dev iteration: never let the browser heuristically cache the engine —
    # a stale wasm next to fresh page sources produces maddening mismatches.
    response.headers["Cache-Control"] = "no-store"
    return response


def serve(path: Path, media: str = "application/octet-stream") -> Response:
    if not path.is_file():
        raise HTTPException(404)
    return FileResponse(path, media_type=media)


@app.get("/")
@app.get("/index.html")
def index():
    return serve(WEB / "index.html", "text/html; charset=utf-8")


@app.get("/{name}.{ext}")
def web_file(name: str, ext: str):
    media = {
        "js": "text/javascript; charset=utf-8",
        "mjs": "text/javascript; charset=utf-8",
        "html": "text/html; charset=utf-8",
        "css": "text/css; charset=utf-8",
        "wasm": "application/wasm",
    }.get(ext, "application/octet-stream")
    return serve(WEB / f"{name}.{ext}", media)


@app.get("/pkg/{name}.{ext}")
def pkg_file(name: str, ext: str):
    media = {
        "js": "text/javascript; charset=utf-8",
        "wasm": "application/wasm",
        "d.ts": "text/plain; charset=utf-8",
    }.get(ext, "application/octet-stream")
    if ext == "d.ts":  # wasm-bindgen also writes platform_web_bg.wasm.d.ts
        return serve(WEB / "pkg" / f"{name}.{ext}", media)
    return serve(WEB / "pkg" / f"{name}.{ext}", media)


@app.get("/dist")
@app.get("/dist/")
@app.get("/dist/index.html")
def dist_index():
    return serve(ROOT / "dist" / "index.html", "text/html; charset=utf-8")


@app.get("/dist/{name}.{ext}")
def dist_file(name: str, ext: str):
    media = {
        "js": "text/javascript; charset=utf-8",
        "mjs": "text/javascript; charset=utf-8",
        "html": "text/html; charset=utf-8",
        "wasm": "application/wasm",
        "d.ts": "text/plain; charset=utf-8",
    }.get(ext, "application/octet-stream")
    if ext == "d.ts":
        return serve(ROOT / "dist" / f"{name}.{ext}", media)
    if name == "platform_web_bg.wasm":
        return serve(ROOT / "dist" / f"{name}.{ext}", "application/wasm")
    return serve(ROOT / "dist" / f"{name}.{ext}", media)


@app.get("/dist/pkg/{name}.{ext}")
def dist_pkg_file(name: str, ext: str):
    media = {
        "js": "text/javascript; charset=utf-8",
        "wasm": "application/wasm",
        "d.ts": "text/plain; charset=utf-8",
    }.get(ext, "application/octet-stream")
    return serve(ROOT / "dist" / "pkg" / f"{name}.{ext}", media)


def main() -> None:
    parser = argparse.ArgumentParser()
    parser.add_argument("--port", type=int, default=8000)
    args = parser.parse_args()
    print(f"serving {WEB} on http://127.0.0.1:{args.port}/ (COOP/COEP on)")
    uvicorn.run(app, host="127.0.0.1", port=args.port, log_level="warning")


if __name__ == "__main__":
    main()

# ReMajiro
Rust-compatible implementation of the Majiro Engine

## Features

- Runs games built on the Majiro Engine directly from their original game archives
- One codebase, two backends: native desktop (winit + wgpu) and WebAssembly (wasm32) for in-browser(Chrome) play
- High-resolution resource patches — drop-in upscaled asset packs at any integer scale (2x, 3x, ...) layered on top of the original game data
- Script IR patching — translations as annotations over original scripts (IRv1, save-compatible), or structurally modified scripts with automatic address relocation (IRv2, experimental)
- Save compatibility with the original engine — saves are interchangeable in both directions
- Per-game build profiles; ships with multi build targets

## Tested Platforms

| Platform | Native | WASM |
| --- | --- | --- |
| Windows | ✅ | ✅ |
| Android | — | ✅ |
| macOS | — | ✅ |
| Linux (Ubuntu) | — | ✅ |

## Tested Games

- 『終わる世界とバースデイ』 (*Owaru Sekai to Birthday* — The end of the world, and happy birthday)
- 『ルリのかさね ～いもうと物語り～』 (*Ruri no Kasane: Imouto Monogatari*)
- 『終わる世界と双子座のパラダイス』 (*Owaru Sekai to Futagoza no Paradise*)

## Getting Started

Prerequisites: a stable [Rust](https://rustup.rs) toolchain. The WASM target additionally needs `rustup target add wasm32-unknown-unknown` and `wasm-bindgen-cli` pinned to `0.2.128` (the version the glue ABI is generated against).

### Native (Windows)

```sh
cargo build --release --bin owarusekai --bin ruri --bin paradise
```

### WebAssembly

```sh
cargo build --release --target wasm32-unknown-unknown -p platform-web
wasm-bindgen target/wasm32-unknown-unknown/release/platform_web.wasm --out-dir web/pkg --target web
```

The browser front-end shell lives in `web/` (`index.html`, `main.js`, `worker.js`); the `wasm-bindgen` output above lands in `web/pkg/`.

The browser build requires cross-origin isolation (COOP/COEP headers) so that `SharedArrayBuffer` is available. `python_scripts/dev_server.py` is a minimal reference server that adds them:

```sh
pip install fastapi uvicorn
python python_scripts/dev_server.py --port 8000
```

Pre-built server binaries for common platforms may be provided later. IR and patch creation tools live in the separate [ReMajiroTools](https://github.com/luoyily/ReMajiroTools) repository.

## Legal Disclaimer

**Non-affiliation.** This project is an independent open-source reimplementation. It is not affiliated with, endorsed by, or sponsored by the original publishers or rights holders in any way.

**Trademarks.** All trademarks, logos, and brand names are the property of their respective owners. The names used here are referenced solely for compatibility identification and educational purposes, and fall under descriptive fair use.

**No copyrighted assets included.** This repository contains only source code written from scratch. It does not include, and must not be distributed with, any copyrighted game assets (audio, images, scripts, or proprietary binaries).

**Requirements to run.** To use this engine to play a game, you must possess a legally obtained copy of the original game (e.g. original physical media or an official digital download).

**Purpose.** This project is intended for technical research, software preservation, and educational purposes only. It must not be used for commercial purposes. The developers assume no responsibility for any actions taken by users of this software.

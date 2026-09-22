// Main-thread page logic: boot flow, IO service, input forwarding, rAF ticks.
// Protocol mirrors crates/platform-web/src/bridge.rs — keep both in sync.

const WINDOW_OFFSET = 64;
const SEQ = 0, STATUS = 1, LEN = 2;

let worker = null;
let engineReady = false;
let dirHandle = null;
let ints = null; // i32 view of the SAB header
let currentGame = null;
let muted = false;

const statusEl = document.getElementById("status");
const bootEl = document.getElementById("boot");
const canvasEl = document.getElementById("screen");
const stageEl = document.getElementById("stage");
const frameEl = document.getElementById("frame");
const toolbarEl = document.getElementById("toolbar");

function setStatus(text) {
  statusEl.textContent = text;
  console.log("[SHELL]", text);
}

// ── Boot ─────────────────────────────────────────────────────────────────────

async function pickAndBoot(game) {
  currentGame = game;
  setStatus("等待浏览器文件夹选择…");
  if (!("showDirectoryPicker" in window)) {
    setStatus("此浏览器不支持 showDirectoryPicker（需要 Chrome）");
    return;
  }
  try {
    dirHandle = await window.showDirectoryPicker({ mode: "read" });
  } catch (error) {
    setStatus("已取消文件夹选择");
    return; // user cancelled
  }
  setStatus("正在扫描目录…");
  let files, patchFiles;
  try {
    files = [];
    patchFiles = [];
    await walkDirectory(dirHandle, "", files, patchFiles);
  } catch (error) {
    setStatus("目录扫描失败: " + error);
    return;
  }
  const hasFont = files.some(([name]) => name === "font.ttf" || name === "font.ttc");
  if (!hasFont && patchFiles.length === 0) {
    setStatus("未找到 font.ttf / font.ttc（需放在游戏目录根下）");
    return;
  }
  setStatus("正在启动引擎… (files=" + files.length + ", patch=" + patchFiles.length + ")");

  const sab = new SharedArrayBuffer(WINDOW_OFFSET + 32 * 1024 * 1024);
  ints = new Int32Array(sab);
  window.__sab = sab; // held for the IO service closures
  worker = new Worker("worker.js", { type: "module" });
  wireWorkerMessages();
  const offscreen = canvasEl.transferControlToOffscreen();
  worker.postMessage(
    {
      t: "boot",
      game,
      files: JSON.stringify(files),
      patchFiles: JSON.stringify(patchFiles),
      sab,
      canvas: offscreen,
    },
    [offscreen],
  );
}

// Recursive enumeration (name, size) pairs. Directories carrying a
// `patch.toml` manifest are collected into a separate listing for the patch
// bundle (R6) and pruned from the game VFS — presentation patches stay
// outside the game namespace, mirroring the desktop scanner.
async function walkDirectory(dir, prefix, gameFiles, patchFiles) {
  for await (const [name, handle] of dir.entries()) {
    if (handle.kind === "file") {
      const file = await handle.getFile();
      gameFiles.push([prefix + name, file.size]);
    } else {
      let isPatch = false;
      try {
        await handle.getFileHandle("patch.toml");
        isPatch = true;
      } catch {}
      if (isPatch) {
        console.log("[SHELL] collecting patch directory", prefix + name);
        await walkPatch(handle, prefix + name + "/", patchFiles);
        console.log("[SHELL] patch listing:", JSON.stringify(patchFiles));
      } else {
        await walkDirectory(handle, prefix + name + "/", gameFiles, patchFiles);
      }
    }
  }
}

async function walkPatch(dir, prefix, out) {
  for await (const [name, handle] of dir.entries()) {
    if (handle.kind === "file") {
      const file = await handle.getFile();
      out.push([prefix + name, file.size]);
    } else {
      await walkPatch(handle, prefix + name + "/", out);
    }
  }
}

// ── IO service (serves the worker's blocking reads / stat / msgbox) ─────────

function wireWorkerMessages() {
  const sab = window.__sab;
  const bytes = new Uint8Array(sab);
  let reads = 0, worstReadMs = 0;
  worker.onmessage = async (e) => {
    const msg = e.data;
    if (msg.t === "read") {
      let status = 1, len = 0;
      const started = performance.now();
      try {
        const file = await openFile(msg.name);
        const slice = file.slice(Number(msg.off), Number(msg.off) + msg.len);
        const buf = new Uint8Array(await slice.arrayBuffer());
        bytes.set(buf, WINDOW_OFFSET);
        len = buf.byteLength;
      } catch (error) {
        // Not-found / revoked permission → STATUS 0 (the engine maps this to
        // the native missing-file path).
        console.warn("[IO] read failed:", msg.name, error);
        status = 0;
      }
      const tookMs = performance.now() - started;
      if (tookMs > worstReadMs) worstReadMs = tookMs;
      if (++reads % 25 === 0) {
        console.log(`[IO] ${reads} reads served, last ${tookMs.toFixed(1)}ms, worst ${worstReadMs.toFixed(1)}ms`);
      }
      Atomics.store(ints, LEN, len);
      Atomics.store(ints, STATUS, status);
      Atomics.store(ints, SEQ, msg.id);
      Atomics.notify(ints, SEQ, 1);
    } else if (msg.t === "stat") {
      let status = 1;
      try {
        const file = await openFile(msg.name);
        Atomics.store(ints, LEN, Number(file.size) | 0);
      } catch (error) {
        console.warn("[IO] stat failed:", msg.name, error);
        status = 0;
      }
      Atomics.store(ints, STATUS, status);
      Atomics.store(ints, SEQ, msg.id);
      Atomics.notify(ints, SEQ, 1);
    } else if (msg.t === "msgbox") {
      alert(msg.text);
      Atomics.store(ints, STATUS, 1);
      Atomics.store(ints, SEQ, msg.id);
      Atomics.notify(ints, SEQ, 1);
    } else if (msg.t === "save-data") {
      finishExport(msg.files);
    } else if (AUDIO_COMMANDS.has(msg.t)) {
      handleAudioCommand(msg);
    } else if (msg.t === "ready") {
      engineReady = true;
      bootEl.style.display = "none";
      stageEl.style.display = "block";
      toolbarEl.style.display = "block";
      setStatus("");
      tickPending = false;
      requestAnimationFrame(tick);
    } else if (msg.t === "tick-done") {
      // Stop-and-wait flow control: the previous engine tick finished.
      // Scheduling the next rAF only here keeps at most one tick in the
      // worker's queue, so input events (e.g. the ctrl keyup that ends a
      // fast-forward) are always processed before further VM work instead
      // of piling up behind backlogged ticks during heavy scene loads.
      tickPending = false;
    } else if (msg.t === "fullscreen") {
      // The engine's in-game fullscreen toggle arrives as a frame outcome
      // (worker glue forwards on_tick's return). Requires transient user
      // activation — normally the same click that triggered it in-game.
      if (msg.enter && !document.fullscreenElement) {
        stageEl.requestFullscreen().catch((error) =>
          console.warn("[SHELL] in-game fullscreen denied:", error));
      } else if (!msg.enter && document.fullscreenElement) {
        document.exitFullscreen();
      }
    } else if (msg.t === "movie-play" || msg.t === "movie-stop") {
      handleMovieCommand(msg);
    } else if (msg.t === "save-list") {
      renderSaveList(msg.files);
    } else if (msg.t === "error") {
      setStatus("启动失败: " + msg.message);
    }
  };
}

async function openFile(name) {
  const parts = name.split("/");
  let entry = dirHandle;
  for (const part of parts.slice(0, -1)) {
    entry = await entry.getDirectoryHandle(part);
  }
  const fh = await entry.getFileHandle(parts[parts.length - 1]);
  return fh.getFile();
}

// ── Input + rAF ──────────────────────────────────────────────────────────────

// One engine tick may be in flight; the worker's `tick-done` ack re-arms the
// rAF loop (see the message handler above).
let tickPending = false;

function tick() {
  if (!tickPending) {
    tickPending = true;
    worker.postMessage({ t: "tick" });
  }
  const now = performance.now();
  if (tick.last) {
    const delta = now - tick.last;
    tick.frames = (tick.frames || 0) + 1;
    tick.totalMs = (tick.totalMs || 0) + delta;
    if (tick.totalMs >= 5000) {
      const fps = (tick.frames * 1000) / tick.totalMs;
      console.log(`[PERF] rAF ${fps.toFixed(1)} fps over ${tick.totalMs.toFixed(0)}ms`);
      tick.frames = 0;
      tick.totalMs = 0;
    }
  }
  tick.last = now;
  requestAnimationFrame(tick);
}

function send(msg) {
  if (engineReady) worker.postMessage(msg);
}

function installInput() {
  window.addEventListener("keydown", (e) => {
    if (["ArrowUp", "ArrowDown", "ArrowLeft", "ArrowRight", "Space", "Tab"].includes(e.code)) {
      e.preventDefault();
    }
    send({ t: "key", code: e.code, pressed: true });
  });
  window.addEventListener("keyup", (e) => {
    send({ t: "key", code: e.code, pressed: false });
  });
  canvasEl.addEventListener("pointermove", (e) => {
    const rect = canvasEl.getBoundingClientRect();
    send({ t: "pmove", x: e.clientX - rect.left, y: e.clientY - rect.top,
           w: rect.width, h: rect.height });
  });
  canvasEl.addEventListener("pointerdown", (e) => {
    const rect = canvasEl.getBoundingClientRect();
    send({ t: "pbutton", button: e.button, pressed: true,
           x: e.clientX - rect.left, y: e.clientY - rect.top,
           w: rect.width, h: rect.height });
    e.preventDefault();
  });
  canvasEl.addEventListener("pointerup", (e) => {
    const rect = canvasEl.getBoundingClientRect();
    send({ t: "pbutton", button: e.button, pressed: false,
           x: e.clientX - rect.left, y: e.clientY - rect.top,
           w: rect.width, h: rect.height });
  });
  canvasEl.addEventListener("wheel", (e) => {
    e.preventDefault();
    send({ t: "wheel", dy: e.deltaY });
  }, { passive: false });
  // The game area uses the right button as input — no browser context menu.
  stageEl.addEventListener("contextmenu", (e) => e.preventDefault());
  window.addEventListener("focus", () => send({ t: "focus", focused: true }));
  window.addEventListener("blur", () => send({ t: "focus", focused: false }));
  window.addEventListener("beforeunload", () => send({ t: "persist" }));
}

// Fullscreen is canvas-only (`#stage` wrapper: black screen, aspect-preserved
// picture, no toolbar) rather than whole-page.
function toggleFullscreen() {
  if (document.fullscreenElement) {
    document.exitFullscreen();
  } else {
    stageEl.requestFullscreen().catch((error) =>
      console.warn("[SHELL] fullscreen denied:", error));
  }
}

function syncFullscreenButton() {
  document.getElementById("fullscreen").textContent =
    document.fullscreenElement ? "退出全屏" : "全屏";
}

// Scale the canvas up to the stage while preserving aspect ratio. Done in JS
// (CSS max-* only ever shrinks) so the element box equals the drawn picture —
// the pointer handlers' rect-relative mapping stays exact.
function fitCanvasToStage() {
  if (!document.fullscreenElement) {
    canvasEl.style.width = "";
    canvasEl.style.height = "";
    return;
  }
  const scale = Math.min(
    stageEl.clientWidth / canvasEl.width,
    stageEl.clientHeight / canvasEl.height,
  );
  canvasEl.style.width = Math.floor(canvasEl.width * scale) + "px";
  canvasEl.style.height = Math.floor(canvasEl.height * scale) + "px";
}

document.getElementById("pick-owarusekai").addEventListener("click", () => pickAndBoot("owarusekai"));
document.getElementById("pick-ruri").addEventListener("click", () => pickAndBoot("ruri"));
document.getElementById("pick-paradise").addEventListener("click", () => pickAndBoot("paradise"));
document.getElementById("fullscreen").addEventListener("click", toggleFullscreen);
document.addEventListener("fullscreenchange", () => {
  syncFullscreenButton();
  fitCanvasToStage();
});
window.addEventListener("resize", fitCanvasToStage);
document.getElementById("mute").addEventListener("click", (event) => {
  muted = !muted;
  event.target.textContent = muted ? "取消静音" : "静音";
  // The page owns the real AudioContext — mute is a direct master-gain set.
  ensureAudio();
  if (audioCtx.state === "suspended") audioCtx.resume();
  masterGain.gain.value = muted ? 0 : 1;
  if (movieEl) movieEl.muted = muted;
});

// ── Movie overlay (R7a): the page owns the real <video> playback ────────────
// Browsers can't decode the games' MPEG-1 movies, so the engine resolves a
// re-encoded H.264/AAC .mp4 twin (web prefers .mp4 over .mpg) and this side
// plays it in an element overlaid exactly on the canvas box (#frame).
// Completion — and any playback error, mirroring the desktop decoder-failure
// path — is reported back so the engine session finishes and the VM resumes.
// A generation counter keeps the async open/play path from reporting for a
// movie that was stopped or replaced in the meantime.

let movieEl = null;
let movieUrl = null;
let movieGeneration = 0;

function handleMovieCommand(msg) {
  if (msg.t === "movie-play") playMovie(msg.name);
  else if (msg.t === "movie-stop") {
    console.log("[MOVIE] engine requests stop");
    stopMovie();
  }
}

async function playMovie(name) {
  const generation = ++movieGeneration;
  teardownMovieElement();
  console.log("[MOVIE] engine requests", name);
  let file;
  try {
    file = await openFile(name);
  } catch (error) {
    console.warn("[MOVIE] file not found:", name, error);
    movieFinished(generation, 0);
    return;
  }
  if (generation !== movieGeneration) return; // stopped/replaced while opening
  const video = document.createElement("video");
  video.id = "movie";
  video.muted = muted;
  video.playsInline = true;
  video.addEventListener("ended", () => movieFinished(generation, video.duration * 1000 || 0));
  video.addEventListener("error", () => {
    console.warn("[MOVIE] playback failed:", name, video.error && video.error.message);
    movieFinished(generation, 0);
  });
  movieEl = video;
  movieUrl = URL.createObjectURL(file);
  video.src = movieUrl;
  frameEl.appendChild(video);
  try {
    await video.play();
    console.log("[MOVIE] playing", name);
  } catch (error) {
    console.warn("[MOVIE] play() rejected:", error);
    movieFinished(generation, 0);
  }
}

function movieFinished(generation, durationMs) {
  if (generation !== movieGeneration) return;
  // Consume the generation: the pending play() promise still rejects with
  // AbortError after this teardown, and that rejection must not re-report.
  movieGeneration += 1;
  console.log("[MOVIE] finished, reporting ms=", durationMs);
  worker.postMessage({ t: "movie-ended", ms: durationMs });
  teardownMovieElement();
}

function stopMovie() {
  movieGeneration++;
  teardownMovieElement();
}

function teardownMovieElement() {
  if (movieEl) {
    movieEl.pause();
    movieEl.remove();
    movieEl = null;
  }
  if (movieUrl) {
    URL.revokeObjectURL(movieUrl);
    movieUrl = null;
  }
}

installInput();
installSaveManager();

// ── Audio service (R4): the page owns the only real AudioContext ────────────
// Chrome hides AudioContext from dedicated workers, so the worker's
// audio_web.rs is a command+mirror adapter: it posts {t:"aplay"|...} here
// and answers the engine's position/drain polls from its own clock. This
// side builds the WebAudio graph with the same shapes as the desktop
// rodio backend (primary-once + looping tail / looped single / one-shot,
// DirectSound pan curve, linear volume).

const AUDIO_COMMANDS = new Set(["aplay", "avol", "apan", "aspd", "apause", "aseek", "astop"]);
let audioCtx = null;
let masterGain = null;
const audioStreams = new Map();

function ensureAudio() {
  if (!audioCtx) {
    audioCtx = new AudioContext();
    masterGain = audioCtx.createGain();
    masterGain.gain.value = muted ? 0 : 1;
    masterGain.connect(audioCtx.destination);
  }
}

["pointerdown", "keydown"].forEach((ev) =>
  window.addEventListener(ev, () => {
    ensureAudio();
    if (audioCtx.state === "suspended") audioCtx.resume();
  }),
);

function panGains(pan) {
  pan = Math.max(-10000, Math.min(10000, pan));
  if (pan < 0) return [1.0, Math.pow(10, pan / 2000)];
  if (pan > 0) return [Math.pow(10, -pan / 2000), 1.0];
  return [1.0, 1.0];
}

async function decodeAudio(data) {
  // decodeAudioData detaches its input; hand it a private copy.
  const copy = data.buffer.slice(data.byteOffset, data.byteOffset + data.byteLength);
  return audioCtx.decodeAudioData(copy);
}

function buildPanGraph(pan) {
  const gainL = audioCtx.createGain();
  const gainR = audioCtx.createGain();
  const merger = audioCtx.createChannelMerger(2);
  const [gl, gr] = panGains(pan);
  gainL.gain.value = gl;
  gainR.gain.value = gr;
  gainL.connect(merger, 0, 0);
  gainR.connect(merger, 0, 1);
  merger.connect(masterGain);
  return { gainL, gainR };
}

function wireSource(stream, source, channels) {
  if (channels === 1) {
    source.connect(stream.graph.gainL);
    source.connect(stream.graph.gainR);
  } else {
    const splitter = audioCtx.createChannelSplitter(2);
    source.connect(splitter);
    splitter.connect(stream.graph.gainL, 0);
    splitter.connect(stream.graph.gainR, 1);
  }
}

function posOf(stream) {
  return stream.base + (audioCtx.currentTime - stream.startedAt) * stream.speed;
}

function stopNodes(stream) {
  stream.nodes.forEach((node) => {
    try { node.stop(); } catch {}
    try { node.disconnect(); } catch {}
  });
  stream.nodes = [];
}

// (Re)schedule the source queue from `offsetSec` — seek/unpause/speed all
// funnel here, mirroring the worker mirror's clock rebases.
function scheduleFrom(stream, offsetSec) {
  stopNodes(stream);
  stream.base = offsetSec;
  stream.startedAt = audioCtx.currentTime;
  const primaryLen = stream.primaryBuf.duration;
  const src = audioCtx.createBufferSource();
  src.buffer = stream.primaryBuf;
  src.playbackRate.value = stream.speed;
  wireSource(stream, src, stream.primaryBuf.numberOfChannels);
  stream.nodes.push(src);

  if (stream.looped) {
    if (stream.tailBuf || offsetSec < primaryLen) {
      const from = Math.min(offsetSec, primaryLen);
      const playLen = primaryLen - from;
      src.start(audioCtx.currentTime, from);
      if (stream.tailBuf) {
        const tail = audioCtx.createBufferSource();
        tail.buffer = stream.tailBuf;
        tail.playbackRate.value = stream.speed;
        tail.loop = true;
        wireSource(stream, tail, stream.tailBuf.numberOfChannels);
        stream.nodes.push(tail);
        const tailFrom = Math.max(0, offsetSec - primaryLen) % Math.max(stream.tailBuf.duration, 1);
        tail.start(audioCtx.currentTime + playLen / stream.speed, tailFrom);
      }
    } else {
      src.loop = true;
      src.start(audioCtx.currentTime, offsetSec % Math.max(primaryLen, 1));
    }
  } else {
    src.start(audioCtx.currentTime, Math.min(offsetSec, Math.max(primaryLen - 0.001, 0)));
    src.onended = () => worker.postMessage({ t: "aended", id: stream.id });
  }
}

function applyGains(stream) {
  const [gl, gr] = panGains(stream.pan);
  stream.graph.gainL.gain.value = stream.vol * gl;
  stream.graph.gainR.gain.value = stream.vol * gr;
}

function handleAudioCommand(msg) {
  switch (msg.t) {
    case "aplay": {
      ensureAudio();
      if (audioCtx.state === "suspended") audioCtx.resume();
      (async () => {
        let primaryBuf, tailBuf = null;
        try {
          primaryBuf = await decodeAudio(msg.data);
          if (msg.tail) tailBuf = await decodeAudio(msg.tail);
        } catch (error) {
          console.warn("[AUDIO] decode failed:", error);
          return;
        }
        const stream = {
          id: msg.id,
          primaryBuf,
          tailBuf,
          looped: msg.looped,
          vol: msg.gain,
          pan: msg.pan,
          speed: 1,
          base: 0,
          startedAt: 0,
          paused: false,
          nodes: [],
          graph: buildPanGraph(msg.pan),
        };
        applyGains(stream);
        audioStreams.set(msg.id, stream);
        scheduleFrom(stream, 0);
      })();
      break;
    }
    case "avol": {
      const stream = audioStreams.get(msg.id);
      if (stream) { stream.vol = msg.v; applyGains(stream); }
      break;
    }
    case "apan": {
      const stream = audioStreams.get(msg.id);
      if (stream) { stream.pan = msg.pan; applyGains(stream); }
      break;
    }
    case "aspd": {
      const stream = audioStreams.get(msg.id);
      if (stream) {
        stream.speed = msg.r;
        stream.nodes.forEach((node) => { node.playbackRate.value = msg.r; });
      }
      break;
    }
    case "apause": {
      const stream = audioStreams.get(msg.id);
      if (!stream) break;
      if (msg.p) {
        stream.base = posOf(stream);
        stream.paused = true;
        stopNodes(stream);
      } else {
        stream.paused = false;
        if (audioCtx.state === "suspended") audioCtx.resume();
        scheduleFrom(stream, stream.base);
      }
      break;
    }
    case "aseek": {
      const stream = audioStreams.get(msg.id);
      if (!stream) break;
      stream.base = msg.ms / 1000;
      if (!stream.paused) scheduleFrom(stream, stream.base);
      break;
    }
    case "astop": {
      const stream = audioStreams.get(msg.id);
      if (stream) stopNodes(stream);
      break;
    }
  }
}

// ── Save manager (web-shell-plan R3): zip export + three-way import ─────────
// Whitelist mirrors platform_web::saves::WebSaveStore::is_save_name.

function isSaveFileName(name) {
  const bare = name.toLowerCase().split("/").pop();
  return (bare.startsWith("majiro_") && bare.endsWith(".sav"))
      || bare === "majiro_system.mss"
      || bare === "majiro_readmark.mss"
      || bare.endsWith("_config.dat");
}

const CRC_TABLE = (() => {
  const table = new Uint32Array(256);
  for (let n = 0; n < 256; n++) {
    let c = n;
    for (let k = 0; k < 8; k++) c = c & 1 ? 0xEDB88320 ^ (c >>> 1) : c >>> 1;
    table[n] = c;
  }
  return table;
})();

function crc32(bytes) {
  let c = 0xFFFFFFFF;
  for (let i = 0; i < bytes.length; i++) c = CRC_TABLE[(c ^ bytes[i]) & 0xFF] ^ (c >>> 8);
  return (c ^ 0xFFFFFFFF) >>> 0;
}

// STORE-only zip: local headers + central directory + EOCD. Extractable by
// Explorer / any unzip tool — the desktop interop contract.
function buildZip(entries) {
  const encoder = new TextEncoder();
  const chunks = [], central = [];
  let offset = 0;
  for (const { name, data } of entries) {
    const nameBytes = encoder.encode(name);
    const crc = crc32(data);
    const local = new DataView(new ArrayBuffer(30));
    local.setUint32(0, 0x04034b50, true);
    local.setUint16(4, 20, true);       // version needed
    local.setUint16(6, 0x0800, true);   // UTF-8 name flag
    local.setUint32(14, crc, true);
    local.setUint32(18, data.length, true);
    local.setUint32(22, data.length, true);
    local.setUint16(26, nameBytes.length, true);
    chunks.push(new Uint8Array(local.buffer), nameBytes, data);
    const cd = new DataView(new ArrayBuffer(46));
    cd.setUint32(0, 0x02014b50, true);
    cd.setUint16(4, 20, true);
    cd.setUint16(6, 20, true);
    cd.setUint16(8, 0x0800, true);
    cd.setUint32(16, crc, true);
    cd.setUint32(20, data.length, true);
    cd.setUint32(24, data.length, true);
    cd.setUint16(28, nameBytes.length, true);
    cd.setUint32(42, offset, true);
    central.push(new Uint8Array(cd.buffer), nameBytes);
    offset += 30 + nameBytes.length + data.length;
  }
  const centralSize = central.reduce((sum, chunk) => sum + chunk.length, 0);
  const eocd = new DataView(new ArrayBuffer(22));
  eocd.setUint32(0, 0x06054b50, true);
  eocd.setUint16(8, entries.length, true);
  eocd.setUint16(10, entries.length, true);
  eocd.setUint32(12, centralSize, true);
  eocd.setUint32(16, offset, true);
  return new Blob([...chunks, ...central, new Uint8Array(eocd.buffer)],
                  { type: "application/zip" });
}

// Zip reader: central-directory walk; STORE entries raw, DEFLATE entries via
// the browser-native DecompressionStream (Explorer-made zips import too).
async function readZip(bytes) {
  const view = new DataView(bytes.buffer, bytes.byteOffset, bytes.byteLength);
  let eocd = -1;
  for (let i = bytes.length - 22; i >= Math.max(0, bytes.length - 22 - 65536); i--) {
    if (view.getUint32(i, true) === 0x06054b50) { eocd = i; break; }
  }
  if (eocd < 0) throw new Error("not a zip");
  const count = view.getUint16(eocd + 10, true);
  let ptr = view.getUint32(eocd + 16, true);
  const out = [];
  for (let i = 0; i < count; i++) {
    if (view.getUint32(ptr, true) !== 0x02014b50) break;
    const method = view.getUint16(ptr + 10, true);
    const compressedSize = view.getUint32(ptr + 20, true);
    const nameLen = view.getUint16(ptr + 28, true);
    const extraLen = view.getUint16(ptr + 30, true);
    const commentLen = view.getUint16(ptr + 32, true);
    const localOffset = view.getUint32(ptr + 42, true);
    const name = new TextDecoder().decode(bytes.subarray(ptr + 46, ptr + 46 + nameLen));
    const localNameLen = view.getUint16(localOffset + 26, true);
    const localExtraLen = view.getUint16(localOffset + 28, true);
    const dataStart = localOffset + 30 + localNameLen + localExtraLen;
    let data = bytes.subarray(dataStart, dataStart + compressedSize);
    if (method === 8) {
      const stream = new Blob([data]).stream()
        .pipeThrough(new DecompressionStream("deflate-raw"));
      data = new Uint8Array(await new Response(stream).arrayBuffer());
    } else {
      data = data.slice();
    }
    if (!name.endsWith("/")) out.push({ name, data });
    ptr += 46 + nameLen + extraLen + commentLen;
  }
  return out;
}

function installSaveManager() {
  const panel = document.getElementById("save-panel");
  const saveStatus = document.getElementById("save-status");
  document.getElementById("save-manager").addEventListener("click", () => {
    const show = panel.style.display !== "block";
    panel.style.display = show ? "block" : "none";
    if (show && engineReady) worker.postMessage({ t: "list-saves" });
  });

  document.getElementById("export-saves").addEventListener("click", () => {
    if (!engineReady) return;
    saveStatus.textContent = "正在导出…";
    worker.postMessage({ t: "export-saves" });
  });

  document.getElementById("import-input").addEventListener("change", async (event) => {
    if (!engineReady) return;
    let imported = 0, skipped = 0;
    for (const file of event.target.files) {
      const bytes = new Uint8Array(await file.arrayBuffer());
      let entries;
      if (file.name.toLowerCase().endsWith(".zip")) {
        try {
          entries = await readZip(bytes);
        } catch (error) {
          saveStatus.textContent = `${file.name}: 不是有效的 zip（${error}）`;
          continue;
        }
      } else {
        entries = [{ name: file.name, data: bytes }];
      }
      for (const { name, data } of entries) {
        const bare = name.split("/").pop();
        if (!isSaveFileName(bare) || data.length === 0) {
          skipped++;
          continue;
        }
        worker.postMessage({ t: "import-save", name: bare, data }, [data.buffer]);
        imported++;
      }
    }
    event.target.value = "";
    saveStatus.textContent =
      `已导入 ${imported} 个文件${skipped ? `，跳过 ${skipped} 个` : ""}。` +
      "槽位立即可见；系统状态需刷新页面后生效。";
    if (imported) worker.postMessage({ t: "list-saves" });
  });
}

// Worker replied to list-saves: render name + size + a delete button.
function renderSaveList(files) {
  const saveStatus = document.getElementById("save-status");
  const fileList = document.getElementById("save-files");
  fileList.textContent = "";
  for (const [name, size] of files) {
    const row = document.createElement("div");
    const label = document.createElement("span");
    label.textContent = `${name}（${(size / 1024).toFixed(1)} KB）`;
    const button = document.createElement("button");
    button.textContent = "删除";
    button.style.marginLeft = "0.5rem";
    button.addEventListener("click", () => {
      worker.postMessage({ t: "delete-save", name });
      row.remove();
      saveStatus.textContent = `已删除 ${name}。`;
    });
    row.appendChild(label);
    row.appendChild(button);
    fileList.appendChild(row);
  }
  if (!files.length) {
    fileList.textContent = "（暂无存档文件）";
  }
}

// Worker replied to {t:'export-saves'}: pack [name, Uint8Array] pairs into a
// STORE zip (Explorer-extractable) plus a manifest, and download it.
function finishExport(files) {
  const saveStatus = document.getElementById("save-status");
  const entries = Array.from(files, ([name, data]) => ({ name, data: new Uint8Array(data) }));
  entries.push({
    name: "manifest.json",
    data: new TextEncoder().encode(JSON.stringify({
      format: "majiro-web-saves-v1",
      game: currentGame,
      exported: new Date().toISOString(),
    })),
  });
  const stamp = new Date().toISOString().slice(0, 16).replace(/[-:T]/g, "");
  const url = URL.createObjectURL(buildZip(entries));
  const anchor = document.createElement("a");
  anchor.href = url;
  anchor.download = `${currentGame}-saves-${stamp}.zip`;
  anchor.click();
  URL.revokeObjectURL(url);
  saveStatus.textContent = `已导出 ${entries.length - 1} 个存档文件。`;
}

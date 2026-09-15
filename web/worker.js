// Dedicated-worker glue: hosts the wasm engine and forwards page messages.
// Boot order: main posts {t:'boot', canvas(transferred), sab, game, files} —
// this worker constructs the EngineWorker (SAB bridge + remote VFS), seeds
// the save mirror from OPFS, awaits the async boot (font, WebGPU adapter, VM
// bootstrap), then announces ready. After that every
// {t:'tick'|'key'|...} message is forwarded into Rust.

import init, { EngineWorker } from "./pkg/platform_web.js?v=2";

let engine = null;
let saveGame = null;

// ── Save persistence: OPFS write-through (web-shell-plan R3) ────────────────
// Rust's WebSaveStore mutates its mirror synchronously, then calls
// `__savePersist(name, bytesOrNull)`. Files live under `saves/<game>/` for
// per-game isolation (same origin serves both games).
//
// Persistence uses OPFS *sync access handles* (worker-only): handles are
// opened once — at boot for existing files, lazily for new ones — and stay
// cached, so every write after the first open is a synchronous
// write+flush on the calling thread. That makes the beforeunload
// `persist()` (majiro_system.mss) reliable instead of best-effort: the tab
// cannot race away from a synchronous write. Only a file's first-ever
// creation is queued on the promise chain.

const syncHandles = new Map(); // name -> FileSystemSyncAccessHandle
let saveQueue = Promise.resolve();

async function saveDirectory(create) {
  const root = await navigator.storage.getDirectory();
  const saves = await root.getDirectoryHandle("saves", { create });
  return saves.getDirectoryHandle(saveGame, { create });
}

async function openSyncHandle(name) {
  if (syncHandles.has(name)) return syncHandles.get(name);
  const dir = await saveDirectory(true);
  const handle = await dir.getFileHandle(name, { create: true });
  // `await` tolerates both the sync handle and a promise wrapper.
  const access = await handle.createSyncAccessHandle();
  syncHandles.set(name, access);
  return access;
}

self.__savePersist = (name, data) => {
  const cached = syncHandles.get(name);
  if (cached && data !== null) {
    // Hot path: synchronous flush — survives tab close.
    cached.write(data, { at: 0 });
    cached.truncate(data.length);
    cached.flush();
    return;
  }
  saveQueue = saveQueue.then(async () => {
    try {
      if (data === null) {
        const handle = syncHandles.get(name);
        if (handle) { handle.close(); syncHandles.delete(name); }
        const dir = await saveDirectory(true);
        await dir.removeEntry(name).catch(() => {});
        return;
      }
      const handle = await openSyncHandle(name);
      handle.write(data, { at: 0 });
      handle.truncate(data.length);
      handle.flush();
    } catch (error) {
      console.warn("[SAVE] OPFS write-through failed:", name, error);
    }
  });
};

async function loadSavesFromOpfs() {
  const out = [];
  try {
    const dir = await saveDirectory(false);
    for await (const [name, handle] of dir.entries()) {
      if (handle.kind !== "file") continue;
      // Open the sync access handle now and read through it — async reads
      // on a file with an open sync handle are rejected (file is locked),
      // and the cached handle makes every later write synchronous.
      const access = await handle.createSyncAccessHandle();
      syncHandles.set(name, access);
      const size = access.getSize();
      const buffer = new Uint8Array(size);
      access.read(buffer, { at: 0 });
      out.push([name, buffer]);
    }
  } catch {
    // First launch: no `saves/<game>/` directory yet.
  }
  return out;
}

// ── Message routing ──────────────────────────────────────────────────────────

self.onmessage = async (e) => {
  const msg = e.data;
  try {
    if (msg.t === "boot") {
      await init();
      saveGame = msg.game;
      engine = new EngineWorker(
        msg.canvas,
        msg.sab,
        msg.game,
        msg.files,
        msg.patchFiles,
      );
      for (const [name, data] of await loadSavesFromOpfs()) {
        engine.add_save_file(name, data);
      }
      await engine.boot();
      self.postMessage({ t: "ready" });
      return;
    }
    if (msg.t === "export-saves") {
      // Array of [name, Uint8Array] pairs; structured-cloned to the page.
      self.postMessage({ t: "save-data", files: engine.export_saves() });
      return;
    }
    if (msg.t === "list-saves") {
      // Names + sizes only (page renders the manager's file list).
      const files = engine.export_saves();
      const list = Array.from(files, ([name, data]) => [name, data.byteLength]);
      self.postMessage({ t: "save-list", files: list });
      return;
    }
    if (msg.t === "delete-save") {
      engine.delete_save(msg.name);
      return;
    }
    if (msg.t === "import-save") {
      engine.import_save(msg.name, msg.data);
      return;
    }
    if (msg.t === "aended") {
      engine.audio_ended(msg.id);
      return;
    }
    if (msg.t === "movie-ended") {
      // The page's <video> overlay finished (or errored); completes the
      // engine's movie session so the VM resumes.
      engine.movie_ended(msg.ms);
      return;
    }
    if (!engine) return;
    switch (msg.t) {
      case "tick": {
        // Some(true)/Some(false) = the game's in-game fullscreen toggle
        // fired this frame; undefined = nothing to forward. The ack must
        // fire even when on_tick throws, or the page would stop ticking.
        try {
          const fullscreen = engine.on_tick();
          if (fullscreen === true || fullscreen === false) {
            self.postMessage({ t: "fullscreen", enter: fullscreen });
          }
        } finally {
          // Stop-and-wait ack: the page schedules the next rAF tick only
          // after this, bounding the worker queue to one tick so input
          // events never sit behind backlogged engine work.
          self.postMessage({ t: "tick-done" });
        }
        break;
      }
      case "key":
        engine.key(msg.code, msg.pressed);
        break;
      case "pmove":
        engine.pointer_move(msg.x, msg.y, msg.w, msg.h);
        break;
      case "pbutton":
        engine.pointer_button(msg.button, msg.pressed, msg.x, msg.y, msg.w, msg.h);
        break;
      case "wheel":
        engine.wheel(msg.dy);
        break;
      case "focus":
        engine.focus(msg.focused);
        break;
      case "persist":
        engine.persist();
        break;
    }
  } catch (error) {
    console.error("[WORKER]", error);
    self.postMessage({ t: "error", message: String(error) });
  }
};

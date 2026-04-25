import init, { start_rfvp_from_directory } from "./pkg/rfvp.js";

const input = document.getElementById("game-dir");
const rescanButton = document.getElementById("rescan-button");
const statusLine = document.getElementById("status-line");
const errorBox = document.getElementById("error-box");
const grid = document.getElementById("game-grid");
const emptyCard = document.getElementById("empty-card");
const launchOverlay = document.getElementById("launch-overlay");
const launchText = document.getElementById("launch-text");
const libraryScreen = document.getElementById("library-screen");
const playerScreen = document.getElementById("player-screen");
const playerTitle = document.getElementById("player-title");
const exitButton = document.getElementById("exit-button");
const canvas = document.getElementById("rfvp-canvas");

const NLS_OPTIONS = [
  { value: "sjis", label: "SJIS" },
  { value: "gbk", label: "GBK" },
  { value: "utf8", label: "UTF-8" },
];

let wasmInitialized = false;
let selectedFileList = null;
let selectedRootName = "";
let games = [];
let running = false;
let nextFileId = 1;
const fileRegistry = new Map();

globalThis.rfvpReadFileRange = function rfvpReadFileRange(fileId, offset, len) {
  const file = fileRegistry.get(Number(fileId));
  if (!file) {
    throw new Error(`RFVP wasm file id not found: ${fileId}`);
  }

  const start = Number(offset);
  const size = Number(len);
  if (!Number.isFinite(start) || !Number.isFinite(size) || start < 0 || size < 0) {
    throw new Error(`Invalid RFVP wasm file range: id=${fileId} offset=${offset} len=${len}`);
  }
  if (size === 0) {
    return new Uint8Array(0);
  }

  const blob = file.slice(start, start + size);
  const url = URL.createObjectURL(blob);
  try {
    const xhr = new XMLHttpRequest();
    xhr.open("GET", url, false);
    xhr.overrideMimeType("text/plain; charset=x-user-defined");
    xhr.send(null);

    if (xhr.status !== 200 && xhr.status !== 0) {
      throw new Error(`RFVP wasm range read failed: HTTP ${xhr.status}`);
    }

    const text = xhr.responseText || "";
    if (text.length !== size) {
      throw new Error(`RFVP wasm range length mismatch: requested=${size} actual=${text.length}`);
    }

    const out = new Uint8Array(text.length);
    for (let i = 0; i < text.length; i += 1) {
      out[i] = text.charCodeAt(i) & 0xff;
    }
    return out;
  } finally {
    URL.revokeObjectURL(url);
  }
};

function setStatus(text) {
  statusLine.textContent = text;
}

function setError(error) {
  const text = error && error.stack ? error.stack : String(error);
  console.error(error);
  errorBox.textContent = text;
  errorBox.style.display = "block";
}

function clearError() {
  errorBox.textContent = "";
  errorBox.style.display = "none";
}

function showLaunching(text) {
  launchText.textContent = text;
  launchOverlay.style.display = "grid";
}

function hideLaunching() {
  launchOverlay.style.display = "none";
}

function normalizePath(path) {
  return String(path || "")
    .replaceAll("\\\\", "/")
    .replaceAll("\\", "/")
    .split("/")
    .filter(Boolean)
    .join("/");
}

function splitPath(path) {
  return normalizePath(path).split("/").filter(Boolean);
}

function displayNls(nls) {
  return NLS_OPTIONS.find((x) => x.value === nls)?.label || "SJIS";
}

function normalizeNls(value) {
  const v = String(value || "").trim().toLowerCase();
  if (v === "shiftjis" || v === "shift-jis" || v === "sjis") return "sjis";
  if (v === "utf-8" || v === "utf8") return "utf8";
  if (v === "gbk") return "gbk";
  return "sjis";
}

function hashString(s) {
  let h = 2166136261;
  for (let i = 0; i < s.length; i += 1) {
    h ^= s.charCodeAt(i);
    h = Math.imul(h, 16777619);
  }
  return (h >>> 0).toString(16);
}

function savedNlsFor(id) {
  try {
    return normalizeNls(localStorage.getItem(`rfvp.nls.${id}`));
  } catch (_) {
    return "sjis";
  }
}

function saveNlsFor(id, nls) {
  try {
    localStorage.setItem(`rfvp.nls.${id}`, normalizeNls(nls));
  } catch (_) {
    // best-effort
  }
}

function rememberLastPlayed(id) {
  try {
    localStorage.setItem(`rfvp.lastPlayed.${id}`, String(Date.now()));
  } catch (_) {
    // best-effort
  }
}

function getLastPlayed(id) {
  try {
    return Number(localStorage.getItem(`rfvp.lastPlayed.${id}`) || "0") || 0;
  } catch (_) {
    return 0;
  }
}

function rootNameFromFileList(fileList) {
  for (const file of fileList) {
    const rel = normalizePath(file.webkitRelativePath || file.name);
    const parts = splitPath(rel);
    if (parts.length > 0) return parts[0];
  }
  return "Selected Folder";
}

function relativeInsideSelectedRoot(file) {
  const rel = normalizePath(file.webkitRelativePath || file.name);
  const parts = splitPath(rel);
  if (parts.length <= 1) return file.name || rel;
  return parts.slice(1).join("/");
}

function looksLikeGameRoot(entries) {
  let hasRootBin = false;
  let hasRootHcb = false;

  for (const entry of entries) {
    const p = normalizePath(entry.gamePath).toLowerCase();
    const parts = splitPath(p);
    if (parts.length !== 1) continue;

    if (parts[0].endsWith(".bin")) hasRootBin = true;
    if (parts[0].endsWith(".hcb")) hasRootHcb = true;
  }

  return hasRootHcb || hasRootBin;
}

function rootHcbEntry(entries) {
  return entries.find((entry) => {
    const parts = splitPath(entry.gamePath.toLowerCase());
    return parts.length === 1 && parts[0].endsWith(".hcb");
  });
}

function scoreText(s) {
  let score = 0;
  let repl = 0;

  for (const ch of s) {
    if (ch === "\uFFFD") {
      repl += 1;
      continue;
    }

    const cp = ch.codePointAt(0);
    if (cp <= 0x7f) {
      if (/^[A-Za-z0-9]$/.test(ch)) score += 2;
      else if (/\s/.test(ch)) score += 0;
      else score += 1;
      continue;
    }

    if ((cp >= 0x3040 && cp <= 0x30ff) || (cp >= 0x4e00 && cp <= 0x9fff)) {
      score += 3;
    } else {
      score += 1;
    }
  }

  return score - repl * 10;
}

function decodeBytes(bytes, encoding) {
  try {
    return new TextDecoder(encoding, { fatal: false }).decode(bytes);
  } catch (_) {
    return "";
  }
}

async function probeTitleFromHcb(entry) {
  try {
    const data = new Uint8Array(await entry.file.arrayBuffer());
    if (data.length < 4) return null;

    const view = new DataView(data.buffer, data.byteOffset, data.byteLength);
    const sysDescOff = view.getUint32(0, true);
    if (sysDescOff >= data.length) return null;

    let off = sysDescOff;
    if (off + 11 > data.length) return null;
    off += 4; // unknown u32
    off += 2; // unknown u16
    off += 2; // unknown u16
    off += 2; // unknown u16

    const titleLen = view.getUint8(off);
    off += 1;
    if (off + titleLen > data.length) return null;

    let raw = data.slice(off, off + titleLen);
    const nul = raw.indexOf(0);
    if (nul >= 0) raw = raw.slice(0, nul);
    if (raw.length === 0) return null;

    const candidates = [
      decodeBytes(raw, "shift_jis"),
      decodeBytes(raw, "gb18030"),
      decodeBytes(raw, "gbk"),
    ].map((s) => s.trim()).filter(Boolean);

    let best = null;
    for (const candidate of candidates) {
      const score = scoreText(candidate);
      if (!best || score > best.score) {
        best = { score, value: candidate };
      }
    }
    return best ? best.value : null;
  } catch (_) {
    return null;
  }
}

async function buildGamesFromFileList(fileList) {
  const rootName = rootNameFromFileList(fileList);
  selectedRootName = rootName;

  const rootEntries = [];
  for (const file of fileList) {
    const rootRelativePath = relativeInsideSelectedRoot(file);
    if (!rootRelativePath) continue;
    rootEntries.push({ file, gamePath: rootRelativePath });
  }

  if (looksLikeGameRoot(rootEntries)) {
    const id = hashString(`${rootName}:/`);
    const hcb = rootHcbEntry(rootEntries);
    const title = (hcb ? await probeTitleFromHcb(hcb) : null) || rootName;
    return [{
      id,
      title,
      rootPath: rootName,
      nls: savedNlsFor(id),
      entries: rootEntries,
      lastPlayed: getLastPlayed(id),
    }];
  }

  const groups = new Map();
  for (const entry of rootEntries) {
    const parts = splitPath(entry.gamePath);
    if (parts.length < 2) continue;
    const groupName = parts[0];
    const gamePath = parts.slice(1).join("/");
    if (!gamePath) continue;

    if (!groups.has(groupName)) groups.set(groupName, []);
    groups.get(groupName).push({ file: entry.file, gamePath });
  }

  const out = [];
  for (const [groupName, entries] of groups.entries()) {
    if (!looksLikeGameRoot(entries)) continue;

    const id = hashString(`${rootName}:${groupName}`);
    const hcb = rootHcbEntry(entries);
    const title = (hcb ? await probeTitleFromHcb(hcb) : null) || groupName;

    out.push({
      id,
      title,
      rootPath: `${rootName}/${groupName}`,
      nls: savedNlsFor(id),
      entries,
      lastPlayed: getLastPlayed(id),
    });
  }

  out.sort((a, b) => {
    if (a.lastPlayed !== b.lastPlayed) return b.lastPlayed - a.lastPlayed;
    return a.title.localeCompare(b.title);
  });

  return out;
}

function renderLibrary() {
  grid.innerHTML = "";
  emptyCard.style.display = games.length === 0 ? "block" : "none";

  for (const game of games) {
    const tile = document.createElement("article");
    tile.className = "game-tile";

    const poster = document.createElement("div");
    poster.className = "poster";

    const posterTitle = document.createElement("div");
    posterTitle.className = "poster-title";
    posterTitle.textContent = game.title;

    const badge = document.createElement("div");
    badge.className = "nls-badge";
    badge.textContent = displayNls(game.nls);

    poster.append(posterTitle, badge);

    const title = document.createElement("div");
    title.className = "game-title";
    title.textContent = game.title;

    const path = document.createElement("div");
    path.className = "game-path";
    path.textContent = game.rootPath;

    const actions = document.createElement("div");
    actions.className = "tile-actions";

    const play = document.createElement("button");
    play.textContent = "Play";
    play.addEventListener("click", () => launchGame(game));

    const select = document.createElement("select");
    for (const opt of NLS_OPTIONS) {
      const option = document.createElement("option");
      option.value = opt.value;
      option.textContent = opt.label;
      option.selected = opt.value === game.nls;
      select.append(option);
    }
    select.addEventListener("change", () => {
      game.nls = normalizeNls(select.value);
      saveNlsFor(game.id, game.nls);
      badge.textContent = displayNls(game.nls);
    });

    const grow = document.createElement("div");
    grow.className = "grow";

    const remove = document.createElement("button");
    remove.className = "danger";
    remove.textContent = "Remove";
    remove.addEventListener("click", () => {
      games = games.filter((x) => x.id !== game.id);
      renderLibrary();
      setStatus(games.length === 0 ? "No games in library." : `${games.length} game(s) in library.`);
    });

    actions.append(play, select, grow, remove);
    tile.append(poster, title, path, actions);
    grid.append(tile);
  }
}

async function scanCurrentSelection() {
  clearError();

  if (!selectedFileList || selectedFileList.length === 0) {
    games = [];
    renderLibrary();
    setStatus("No folder selected.");
    rescanButton.disabled = true;
    return;
  }

  setStatus(`Scanning ${selectedFileList.length} file(s)...`);
  await new Promise((resolve) => setTimeout(resolve, 0));

  games = await buildGamesFromFileList(selectedFileList);
  renderLibrary();
  rescanButton.disabled = false;

  if (games.length === 0) {
    setStatus(`No valid game root found under ${selectedRootName}.`);
  } else {
    setStatus(`${games.length} game(s) found under ${selectedRootName}.`);
  }
}

function registerGameEntries(game) {
  fileRegistry.clear();

  const files = [];
  for (const entry of game.entries) {
    const id = nextFileId++;
    fileRegistry.set(id, entry.file);
    files.push({
      path: normalizePath(entry.gamePath),
      id,
      size: entry.file.size,
    });
  }

  console.log("RFVP wasm registered files:", files.length);
  console.log("RFVP wasm file sample:", files.slice(0, 50).map((f) => `${f.path} (${f.size})`));
  return files;
}

async function ensureWasmInitialized() {
  if (wasmInitialized) return;
  showLaunching("Loading wasm…");
  await init();
  wasmInitialized = true;
}

async function launchGame(game) {
  if (running) return;

  try {
    clearError();
    running = true;
    showLaunching("Registering files…");

    await ensureWasmInitialized();
    const files = registerGameEntries(game);

    rememberLastPlayed(game.id);
    game.lastPlayed = Date.now();

    libraryScreen.style.display = "none";
    playerScreen.style.display = "block";
    playerTitle.textContent = game.title;
    canvas.focus();

    hideLaunching();

    await start_rfvp_from_directory(
      "rfvp-canvas",
      game.nls,
      JSON.stringify(files),
    );

    setStatus("Running.");
  } catch (error) {
    running = false;
    playerScreen.style.display = "none";
    libraryScreen.style.display = "flex";
    hideLaunching();
    setError(error);
    setStatus("RFVP failed to start.");
  }
}

function exitPlayer() {
  // The current wasm entry owns the engine lifecycle. Until a destroy/stop export exists,
  // returning to the library requires a page reload to guarantee all WebGPU/audio state is reset.
  window.location.reload();
}

input.addEventListener("change", async () => {
  selectedFileList = input.files;
  await scanCurrentSelection();
});

rescanButton.addEventListener("click", async () => {
  await scanCurrentSelection();
});

exitButton.addEventListener("click", exitPlayer);

window.addEventListener("resize", () => {
  if (playerScreen.style.display === "block") {
    canvas.focus();
  }
});

renderLibrary();

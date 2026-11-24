import init, { listing } from "./pkg/rdna2_wasm.js";

const $ = (id) => document.getElementById(id);
const hex = $("hex"), status = $("status"), wave64 = $("wave64");
const table = $("listing"), body = table.querySelector("tbody"), graph = $("graph");

// A small kernel: a loop that waits on memory, a branch around ALU work,
// sampling, a store and an export.
const EXAMPLE = `F4000001 FA000000 BF8C0070 BE800380 80008100 BF0A8800
BF85FFFD BF880003 06000501 D54B8200 240E0501 7E0002FA FF00B101
F0800F0A 00820004 00000006 BF8C0070 E0705010 80010001 F800180F 03020100
BF810000`;

/** The input being shown: bytes and where they came from. */
let current = null;

/** Hex text to bytes: 8-digit tokens are little-endian dwords, 2-digit tokens are bytes. */
function parseHex(text) {
  const bytes = [];
  for (const raw of text.split(/[\s,]+/).filter(Boolean)) {
    const t = raw.replace(/^0x/i, "");
    if (!/^[0-9a-f]+$/i.test(t) || (t.length !== 8 && t.length !== 2)) {
      throw new Error(`"${raw}" is not an 8-digit dword or a 2-digit byte`);
    }
    if (t.length === 2) bytes.push(parseInt(t, 16));
    else for (let i = 6; i >= 0; i -= 2) bytes.push(parseInt(t.slice(i, i + 2), 16));
  }
  return new Uint8Array(bytes);
}

const escape = (s) => s.replace(/[&<>"]/g, (c) => ({ "&": "&amp;", "<": "&lt;", ">": "&gt;", '"': "&quot;" })[c]);

// Registers, then modifiers (`name:` or a bare flag), then numbers.
const TOKEN = new RegExp([
  String.raw`(\b(?:s|v|ttmp)(?:\d+|\[\d+:\d+\])|\b(?:vcc_lo|vcc_hi|vcc|exec_lo|exec_hi|exec|m0|null|scc|off)\b)`,
  String.raw`(\b[a-z_0-9]+(?=[:(])|\b(?:offen|idxen|glc|slc|dlc|gds|lds|tfe|clamp|done|vm|compr|row_mirror|row_half_mirror)\b)`,
  String.raw`(-?\b0x[0-9a-f]+\b|-?\b\d+(?:\.\d+)?\b)`,
].join("|"), "gi");

function highlight(text) {
  if (text.startsWith("<")) return `<span class="unknown">${escape(text)}</span>`;
  const space = text.indexOf(" ");
  const op = space < 0 ? text : text.slice(0, space);
  const rest = space < 0 ? "" : text.slice(space);
  const operands = escape(rest).replace(TOKEN, (m, reg, mod) =>
    `<span class="${reg ? "reg" : mod ? "mod" : "num"}">${m}</span>`);
  return `<span class="op">${escape(op)}</span>${operands}`;
}

/** Parses the WASM records into instructions, symbols and blocks. */
function parse(records) {
  const out = { insts: [], symbols: new Map(), blocks: [], error: null };
  for (const line of records.split("\n").filter(Boolean)) {
    const [kind, ...f] = line.split("\t");
    if (kind === "E") out.error = f[0];
    else if (kind === "S") out.symbols.set(parseInt(f[0], 16), [...(out.symbols.get(parseInt(f[0], 16)) ?? []), f[1]]);
    else if (kind === "I") out.insts.push({ offset: parseInt(f[0], 16), text: f[1], words: f[2] });
    else if (kind === "B") out.blocks.push({ first: +f[0], end: +f[1], succ: f[2] ? f[2].split(",").map(Number) : [] });
  }
  return out;
}

const pad = (n) => n.toString(16).padStart(8, "0");

function renderListing({ insts, symbols, blocks }) {
  const starts = new Set(blocks.map((b) => b.first));
  body.innerHTML = insts.map((inst, i) => {
    const labels = (symbols.get(inst.offset) ?? [])
      .map((name) => `<tr class="label"><td></td><td colspan="2">&lt;${escape(name)}&gt;:</td></tr>`).join("");
    const cls = starts.has(i) && i > 0 ? ' class="block-start"' : "";
    return `${labels}<tr${cls}><td>${pad(inst.offset)}</td><td>${highlight(inst.text)}</td><td>${inst.words}</td></tr>`;
  }).join("");
}

/** Basic blocks stacked in address order, with jumps drawn as arrows in a left gutter. */
function renderGraph({ insts, symbols, blocks }) {
  graph.innerHTML = blocks.map((b, i) => {
    const names = (symbols.get(insts[b.first].offset) ?? []).map((n) => `<span class="sym">${escape(n)}</span>`).join(" ");
    const rows = insts.slice(b.first, b.end)
      .map((inst) => `<div><span class="off">${pad(inst.offset)}</span>${highlight(inst.text)}</div>`).join("");
    const exits = b.succ.length ? `→ ${b.succ.map((s) => `B${s}`).join(", ")}` : "end";
    const falls = b.succ.includes(i + 1) && !insts[b.end - 1].text.startsWith("s_branch");
    return `<div class="block${falls ? " falls" : ""}" id="b${i}"><div class="head"><span>B${i}</span>${names}<span class="exits">${exits}</span></div>${rows}</div>`;
  }).join("");

  // Edges other than falling through to the next block, each on its own lane
  // wherever it would overlap another.
  const edges = [];
  blocks.forEach((b, i) => {
    const last = insts[b.end - 1].text;
    for (const s of b.succ) {
      if (s === i + 1 && !last.startsWith("s_branch")) continue;
      edges.push({ from: i, to: s, kind: s <= i ? "back" : last.startsWith("s_cbranch") ? "cond" : "jump" });
    }
  });
  const box = graph.getBoundingClientRect();
  const cards = blocks.map((_, i) => $(`b${i}`).getBoundingClientRect());
  const lanes = [];
  for (const e of edges.sort((a, b) => Math.abs(a.to - a.from) - Math.abs(b.to - b.from))) {
    e.y1 = cards[e.from].bottom - box.top - 12;
    e.y2 = cards[e.to].top - box.top + 14;
    const [lo, hi] = [Math.min(e.y1, e.y2), Math.max(e.y1, e.y2)];
    e.lane = lanes.findIndex((lane) => lane.every(([a, b]) => hi < a - 4 || lo > b + 4));
    if (e.lane < 0) e.lane = lanes.push([]) - 1;
    lanes[e.lane].push([lo, hi]);
  }
  const gutter = 24 + lanes.length * 14;
  graph.style.paddingLeft = `${gutter}px`;
  // Card positions move with the padding, but only sideways.
  const left = gutter - 4;
  const paths = edges.map((e) => {
    const x = left - 12 - e.lane * 14;
    return `<path class="edge ${e.kind}" d="M${left} ${e.y1} H${x} V${e.y2} H${left - 2}" marker-end="url(#arrow-${e.kind})"/>`;
  }).join("");
  const markers = ["cond", "jump", "back"].map((k) =>
    `<marker id="arrow-${k}" viewBox="0 0 8 8" refX="7" refY="4" markerWidth="7" markerHeight="7" orient="auto"><path class="${k}" d="M0 0L8 4L0 8z"/></marker>`).join("");
  graph.insertAdjacentHTML("afterbegin",
    `<svg class="edges" width="${gutter}" height="${graph.scrollHeight}" aria-hidden="true"><defs>${markers}</defs>${paths}</svg>`);
}

function report(text, error = false) {
  status.textContent = text;
  status.classList.toggle("error", error);
}

let parsed = null;

function render() {
  if (!current) return;
  parsed = parse(listing(current.bytes, wave64.checked));
  if (parsed.error) {
    body.innerHTML = graph.innerHTML = "";
    $("output").hidden = true;
    return report(`${current.name}: ${parsed.error}`, true);
  }
  $("output").hidden = parsed.insts.length === 0;
  renderListing(parsed);
  if (view() === "graph") renderGraph(parsed);
  const { insts, symbols, blocks } = parsed;
  const extra = current.bytes.length % 4 ? `, ${current.bytes.length % 4} trailing byte(s) ignored` : "";
  const syms = symbols.size ? `, ${symbols.size} symbol${symbols.size === 1 ? "" : "s"}` : "";
  report(`${current.name}: ${insts.length} instructions in ${blocks.length} blocks${syms}${extra}`);
}

const view = () => document.querySelector('input[name="view"]:checked').value;

function showView() {
  table.hidden = view() !== "listing";
  graph.hidden = view() !== "graph";
  if (parsed && !parsed.error && view() === "graph") renderGraph(parsed);
}

function runHex() {
  try {
    current = { bytes: parseHex(hex.value), name: "input" };
    render();
  } catch (e) {
    report(e.message, true);
  }
}

async function openFile(file) {
  if (!file) return;
  hex.value = "";
  current = { bytes: new Uint8Array(await file.arrayBuffer()), name: file.name };
  render();
}

await init();
$("run").addEventListener("click", runHex);
$("example").addEventListener("click", () => { hex.value = EXAMPLE; runHex(); });
hex.addEventListener("keydown", (e) => { if (e.key === "Enter" && (e.ctrlKey || e.metaKey)) runHex(); });
$("file").addEventListener("change", (e) => openFile(e.target.files[0]));
wave64.addEventListener("change", render);
for (const r of document.querySelectorAll('input[name="view"]')) r.addEventListener("change", showView);
addEventListener("resize", () => { if (!graph.hidden && parsed) renderGraph(parsed); });
addEventListener("dragover", (e) => { e.preventDefault(); document.body.classList.add("dragging"); });
addEventListener("dragleave", () => document.body.classList.remove("dragging"));
addEventListener("drop", (e) => {
  e.preventDefault();
  document.body.classList.remove("dragging");
  openFile(e.dataTransfer.files[0]);
});
// `#graph` in the address opens the control-flow view.
if (location.hash === "#graph") document.querySelector('input[value="graph"]').checked = true;
showView();
hex.value = EXAMPLE;
runHex();

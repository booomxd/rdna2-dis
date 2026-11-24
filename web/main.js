import init, { disassemble } from "./pkg/rdna2_wasm.js";

const $ = (id) => document.getElementById(id);
const hex = $("hex"), status = $("status"), table = $("listing"), body = table.querySelector("tbody");

// A small shader-like sequence: load, wait, ALU, DPP, sample, store, export.
const EXAMPLE = `F4000001 FA000000 BF8C0070 06000501 7C040300 02000501
D54B8200 240E0501 7E0002FA FF00B101 F0800F0A 00820004 00000006
BF8C0070 DC348008 007D0002 E0705010 80010001 F800180F 03020100 BF810000`;

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

const escape = (s) => s.replace(/[&<>]/g, (c) => ({ "&": "&amp;", "<": "&lt;", ">": "&gt;" })[c]);

// Registers, then modifiers (`name:` or a bare flag), then numbers.
const TOKEN = new RegExp([
  String.raw`(\b(?:s|v|ttmp)(?:\d+|\[\d+:\d+\])|\b(?:vcc_lo|vcc_hi|vcc|exec_lo|exec_hi|exec|m0|null|scc|off)\b)`,
  String.raw`(\b[a-z_0-9]+(?=:)|\b(?:offen|idxen|glc|slc|dlc|gds|lds|tfe|clamp|done|vm|compr|row_mirror|row_half_mirror)\b)`,
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

/** Disassembles `bytes` into the table; returns a status line. */
function show(bytes) {
  const lines = disassemble(bytes).split("\n").filter(Boolean);
  body.innerHTML = lines.map((line) => {
    const [offset, text, words] = line.split("\t");
    return `<tr><td>${offset}</td><td>${highlight(text)}</td><td>${words}</td></tr>`;
  }).join("");
  table.hidden = lines.length === 0;
  const extra = bytes.length % 4 ? `; ${bytes.length % 4} trailing byte(s) ignored` : "";
  return `${lines.length} instruction${lines.length === 1 ? "" : "s"}${extra}`;
}

function report(text, error = false) {
  status.textContent = text;
  status.classList.toggle("error", error);
}

function run() {
  try {
    report(show(parseHex(hex.value)));
  } catch (e) {
    report(e.message, true);
  }
}

async function openFile(file) {
  if (!file) return;
  hex.value = "";
  report(`${file.name}: ${show(new Uint8Array(await file.arrayBuffer()))}`);
}

await init();
$("run").addEventListener("click", run);
$("example").addEventListener("click", () => { hex.value = EXAMPLE; run(); });
hex.addEventListener("keydown", (e) => { if (e.key === "Enter" && (e.ctrlKey || e.metaKey)) run(); });
$("file").addEventListener("change", (e) => openFile(e.target.files[0]));
addEventListener("dragover", (e) => { e.preventDefault(); document.body.classList.add("dragging"); });
addEventListener("dragleave", () => document.body.classList.remove("dragging"));
addEventListener("drop", (e) => {
  e.preventDefault();
  document.body.classList.remove("dragging");
  openFile(e.dataTransfer.files[0]);
});
hex.value = EXAMPLE;
run();

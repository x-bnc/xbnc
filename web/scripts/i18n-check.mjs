// Checks the panel texts against the locale files (../locales/*.json):
// every t("...") / tk("...") / tx("...") of the source must be in the
// "panel" section of en.json (and nothing else), and every other language
// must keep the {placeholders}. Missing translations in other languages are
// listed but allowed: they show in English.
// Exit code 1 on a problem, so CI catches it.
//   node scripts/i18n-check.mjs            report
//   node scripts/i18n-check.mjs --keys     print every key as JSON
//   node scripts/i18n-check.mjs --sync     add new texts to en.json and
//                                          remove the unused ones
import fs from "node:fs";
import path from "node:path";
import ts from "typescript";

function files(dir) {
  return fs.readdirSync(dir, { withFileTypes: true }).flatMap((e) => {
    const p = path.join(dir, e.name);
    if (e.isDirectory()) return files(p);
    return /\.(tsx?|mts)$/.test(e.name) ? [p] : [];
  });
}

const keys = new Set();
for (const f of files("src")) {
  const kind = f.endsWith(".tsx") ? ts.ScriptKind.TSX : ts.ScriptKind.TS;
  const sf = ts.createSourceFile(f, fs.readFileSync(f, "utf8"), ts.ScriptTarget.Latest, true, kind);
  const visit = (n) => {
    if (ts.isCallExpression(n) && ["t", "tk", "tx"].includes(n.expression.getText())) {
      const collect = (a) => {
        if (!a) return;
        if (ts.isStringLiteral(a) || ts.isNoSubstitutionTemplateLiteral(a)) keys.add(a.text);
        else if (ts.isConditionalExpression(a)) {
          collect(a.whenTrue);
          collect(a.whenFalse);
        } else if (ts.isParenthesizedExpression(a)) collect(a.expression);
      };
      collect(n.arguments[0]);
    }
    ts.forEachChild(n, visit);
  };
  visit(sf);
}

// Texts written straight into the markup (not through t/tx) are never
// translated: report JSX text and string branches of conditionals inside
// JSX that look like words.
const raw = [];
const ALLOWED = new Set(["xBNC", "IRC", "TLS", "SSL", "SASL", "API", "OK", "JSON", "IP", "SHA-1", "SHA-256", "SHA-256:", "SHA-512", "sudo xbncd setup-code", "#xbnc"]);
const wordy = (s) => /[a-záéíóúñ]{2,}/i.test(s) && !ALLOWED.has(s.trim());
for (const f of files("src").filter((f) => f.endsWith(".tsx"))) {
  const sf = ts.createSourceFile(f, fs.readFileSync(f, "utf8"), ts.ScriptTarget.Latest, true, ts.ScriptKind.TSX);
  const visit = (n, inJsx) => {
    if (ts.isCallExpression(n) && ["t", "tk", "tx"].includes(n.expression.getText())) return;
    if (ts.isJsxText(n) && wordy(n.text.trim()) && n.text.trim()) {
      raw.push(`${f}:${sf.getLineAndCharacterOfPosition(n.getStart()).line + 1}: ${JSON.stringify(n.text.trim())}`);
    }
    if (inJsx && ts.isConditionalExpression(n)) {
      for (const br of [n.whenTrue, n.whenFalse]) {
        if (ts.isStringLiteral(br) && /[a-záéíóúñ]{3,}/i.test(br.text) && !ALLOWED.has(br.text.trim())) {
          raw.push(`${f}:${sf.getLineAndCharacterOfPosition(br.getStart()).line + 1}: ${JSON.stringify(br.text)}`);
        }
      }
    }
    // Attributes that are not shown to people (className, type, href...).
    if (ts.isJsxAttribute(n) && !["title", "placeholder", "aria-label", "alt", "label"].includes(n.name.getText())) return;
    const jsx = inJsx || ts.isJsxExpression(n);
    ts.forEachChild(n, (c) => visit(c, jsx));
  };
  visit(sf, false);
}

if (process.argv.includes("--keys")) {
  console.log(JSON.stringify([...keys].sort(), null, 1));
  process.exit(0);
}

let bad = false;
const LOCALES = path.join("..", "locales");
const enPath = path.join(LOCALES, "en.json");
const en = JSON.parse(fs.readFileSync(enPath, "utf8"));
if (process.argv.includes("--sync")) {
  const panel = {};
  for (const k of [...keys].sort()) panel[k] = en.panel[k] ?? k.split("|")[0];
  en.panel = panel;
  fs.writeFileSync(enPath, JSON.stringify(en, null, 2) + "\n");
  console.log(`en.json: ${keys.size} panel texts`);
}
const have = new Set(Object.keys(en.panel));
const missing = [...keys].filter((k) => !have.has(k));
const unused = [...have].filter((k) => !keys.has(k));
const ph = (s) => (s.match(/\{\w+\}/g) ?? []).sort().join(",");
console.log(`en.json: ${keys.size} texts in the panel, ${missing.length} missing, ${unused.length} unused`);
for (const k of missing) console.log(`  missing: ${JSON.stringify(k)}  (run with --sync)`);
for (const k of unused) console.log(`  unused: ${JSON.stringify(k)}  (run with --sync)`);
if (missing.length || unused.length) bad = true;
for (const f of fs.readdirSync(LOCALES).filter((f) => f.endsWith(".json") && f !== "en.json")) {
  const cat = JSON.parse(fs.readFileSync(path.join(LOCALES, f), "utf8")).panel ?? {};
  const absent = [...keys].filter((k) => !cat[k]);
  const extra = Object.keys(cat).filter((k) => !keys.has(k));
  const mismatched = Object.keys(cat).filter((k) => keys.has(k) && cat[k] && ph(k.split("|")[0]) !== ph(cat[k]));
  console.log(`${f}: ${keys.size - absent.length} of ${keys.size} translated, ${extra.length} unused, ${mismatched.length} with different placeholders`);
  for (const k of extra) console.log(`  unused: ${JSON.stringify(k)}`);
  for (const k of mismatched) console.log(`  placeholders: ${JSON.stringify(k)}`);
  if (process.argv.includes("--missing")) for (const k of absent) console.log(`  missing: ${JSON.stringify(k)}`);
  if (extra.length || mismatched.length) bad = true;
}
if (raw.length) {
  console.log(`${raw.length} text(s) written outside t()/tx():`);
  for (const r of raw) console.log(`  ${r}`);
  bad = true;
}
process.exit(bad ? 1 : 0);

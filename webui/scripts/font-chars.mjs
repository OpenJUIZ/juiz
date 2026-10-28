// 同梱フォント(src/assets/fonts/)に入れる字の一覧を作る・確かめる(#111)。
//
//   node scripts/font-chars.mjs          字の一覧 src/assets/fonts/subset-chars.txt を書き直す
//   node scripts/font-chars.mjs --check  画面の字がすべて一覧に入っているか確かめる(CI 用)
//
// 画面に出る字は src/ の .ts / .tsx の文字列と JSX の文と index.html から集める(コメントは除く)。
// それに ASCII・かな・和文の約物を足す。一覧を書き直したら scripts/subset-fonts.sh で woff2 を作り直す。
import { readFileSync, readdirSync, writeFileSync } from "node:fs";
import { dirname, extname, join } from "node:path";
import { fileURLToPath } from "node:url";
import ts from "typescript";

const root = join(dirname(fileURLToPath(import.meta.url)), "..");
const listPath = join(root, "src/assets/fonts/subset-chars.txt");

// 画面の文字列ではないので見ない(API の型は生成物、openapi.json は説明文が画面に出ない)
const SKIP_DIRS = new Set([join(root, "src/api")]);

function* sourceFiles(dir) {
  for (const entry of readdirSync(dir, { withFileTypes: true })) {
    const path = join(dir, entry.name);
    if (entry.isDirectory()) {
      if (!SKIP_DIRS.has(path)) yield* sourceFiles(path);
    } else if ([".ts", ".tsx"].includes(extname(path))) {
      yield path;
    }
  }
}

// 文字列リテラル・テンプレート・JSX の文だけを拾う
function textsOf(path) {
  const kind = path.endsWith(".tsx") ? ts.ScriptKind.TSX : ts.ScriptKind.TS;
  const source = ts.createSourceFile(path, readFileSync(path, "utf8"), ts.ScriptTarget.Latest, false, kind);
  const texts = [];
  const visit = (node) => {
    if (
      ts.isStringLiteral(node) ||
      ts.isNoSubstitutionTemplateLiteral(node) ||
      ts.isTemplateHead(node) ||
      ts.isTemplateMiddle(node) ||
      ts.isTemplateTail(node) ||
      ts.isJsxText(node)
    ) {
      texts.push(node.text);
    }
    ts.forEachChild(node, visit);
  };
  visit(source);
  return texts;
}

function range(from, to) {
  return Array.from({ length: to - from + 1 }, (_, i) => String.fromCodePoint(from + i));
}

function collect() {
  const chars = new Set([
    ...range(0x20, 0x7e), // ASCII
    ...range(0x3000, 0x303f), // 和文の約物(、。「」〜 など)
    ...range(0x3041, 0x3096), // ひらがな
    ...range(0x309b, 0x309e),
    ...range(0x30a0, 0x30ff), // カタカナ
    ..."—–…‐‘’“”・×→←↑↓",
  ]);
  const add = (text) => {
    for (const c of text) if (c.codePointAt(0) > 0x7e) chars.add(c);
  };
  for (const path of sourceFiles(join(root, "src"))) textsOf(path).forEach(add);
  add(readFileSync(join(root, "index.html"), "utf8").replace(/<!--[\s\S]*?-->/g, ""));
  chars.delete("\n");
  return [...chars].sort((a, b) => a.codePointAt(0) - b.codePointAt(0));
}

const chars = collect();
if (process.argv.includes("--check")) {
  const listed = new Set(readFileSync(listPath, "utf8"));
  const missing = chars.filter((c) => !listed.has(c));
  if (missing.length > 0) {
    console.error(`同梱フォントに無い字があります: ${missing.join("")}`);
    console.error("npm run fonts:chars で一覧を書き直し、scripts/subset-fonts.sh で woff2 を作り直してください。");
    process.exit(1);
  }
  console.log(`同梱フォントの字は足りています(画面の字 ${chars.length} 字)`);
} else {
  writeFileSync(listPath, chars.join("") + "\n");
  console.log(`${listPath} に ${chars.length} 字を書きました`);
}

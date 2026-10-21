// juiz の OpenAPI 定義から API の型(src/api/openapi.gen.ts)を作る。
//
//   node scripts/gen-api.mjs                         # 保存済みの src/api/openapi.json から作る
//   node scripts/gen-api.mjs http://localhost:8000   # 動いている juiz から取り直して作る
//
// juiz の定義は operationId が API の種類をまたいで重複している(list_dummy など)ため、
// そのままだと型が衝突する。operationId を消してから生成する。
import { readFile, writeFile } from "node:fs/promises";
import openapiTS, { astToString } from "openapi-typescript";

const jsonPath = new URL("../src/api/openapi.json", import.meta.url);
const outPath = new URL("../src/api/openapi.gen.ts", import.meta.url);

const server = process.argv[2];
if (server) {
  const res = await fetch(`${server.replace(/\/+$/, "")}/api-docs/openapi.json`);
  if (!res.ok) throw new Error(`OpenAPI 定義を取得できません: ${res.status}`);
  const fetched = await res.json();
  await writeFile(jsonPath, JSON.stringify(fetched, null, 2) + "\n");
}

const spec = JSON.parse(await readFile(jsonPath, "utf8"));
for (const item of Object.values(spec.paths ?? {})) {
  for (const op of Object.values(item)) {
    if (op && typeof op === "object") delete op.operationId;
  }
}

const ast = await openapiTS(spec);
const header = "// node scripts/gen-api.mjs で生成。直接編集しない。\n\n";
await writeFile(outPath, header + astToString(ast));
console.log(`生成しました: ${outPath.pathname}`);

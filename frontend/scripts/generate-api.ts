import { exec as execCb } from "node:child_process";
import * as fs from "node:fs/promises";
import path from "node:path";
import { promisify } from "node:util";
import openapiTS, { astToString } from "openapi-typescript";

const exec = promisify(execCb);

const WORKSPACE_ROOT = path.resolve(import.meta.dirname, "../../");
const OUT_DIR = path.resolve(import.meta.dirname, "../src/api");
const OUT_FILE = path.join(OUT_DIR, "types.ts");

// runs `cargo run -- open-api` from the workspace root to get the open-api spec.
async function getSpec(): Promise<object> {
  const { stdout } = await exec("cargo run -- open-api", {
    cwd: WORKSPACE_ROOT,
  });
  return JSON.parse(stdout);
}

async function main() {
  console.log("Fetching OpenAPI spec...");
  const spec = await getSpec();

  console.log("Generating TypeScript types...");
  const ast = await openapiTS(spec as Parameters<typeof openapiTS>[0], {
    defaultNonNullable: false,
  });
  const output = astToString(ast);

  await fs.mkdir(OUT_DIR, { recursive: true });
  await fs.writeFile(OUT_FILE, output);
  console.log(`Written to ${OUT_FILE}`);
}

main().catch((e) => {
  console.error(e);
  process.exit(1);
});

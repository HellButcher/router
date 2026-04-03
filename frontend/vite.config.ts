import fs from "node:fs/promises";
import toml from "toml";
import type { UserConfig } from "vite";

// read version from workspace Cargo.toml
async function readCargoVersion() {
  const cargoTomlContent = await fs.readFile("../Cargo.toml", "utf-8");
  const cargoToml = toml.parse(cargoTomlContent);
  return typeof cargoToml.package?.version === "string"
    ? cargoToml.package.version
    : cargoToml.workspace?.package?.version;
}

export default async ({ _command, _mode }) => {
  const version = await readCargoVersion();

  return {
    build: {
      sourcemap: true,
    },
    define: {
      __APP_VERSION__: JSON.stringify(version),
    },
    server: {
      proxy: {
        "/api": {
          target: "http://localhost:8080",
        },
      },
    },
  } satisfies UserConfig;
};

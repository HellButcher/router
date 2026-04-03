import type { UserConfig } from "vite";
import toml from "toml";
import fs from "fs/promises";

// read version from workspace Cargo.toml
async function readCargoVersion() {
  const cargoTomlContent = await fs.readFile("../Cargo.toml", "utf-8");
  const cargoToml = toml.parse(cargoTomlContent);
  return typeof cargoToml.package?.version === "string"
    ? cargoToml.package.version
    : cargoToml.workspace?.package?.version;
}

export default async ({ command, mode }) => {
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

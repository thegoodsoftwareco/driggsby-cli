import { existsSync } from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";
import { spawnFileAndExit } from "./lib/process.js";

const packageDirectory = dirname(fileURLToPath(import.meta.url));
const binaryPath = join(packageDirectory, "node_modules", ".bin_real", "driggsby");

if (!existsSync(binaryPath)) {
  console.error("Driggsby is not installed. Reinstall the npm package and try again.");
  process.exit(1);
}

spawnFileAndExit(binaryPath, process.argv.slice(2));

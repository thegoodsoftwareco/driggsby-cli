import { existsSync } from "node:fs";
import {
  installedBinaryPath,
  readPackageJson,
  resolvePlatform,
  unsupportedPlatformMessage,
} from "./lib/artifacts.js";
import { spawnFileAndExit } from "./lib/process.js";

const packageJson = readPackageJson();
const resolution = resolvePlatform(packageJson);

if (!resolution.platform) {
  console.error(unsupportedPlatformMessage(resolution));
  process.exit(1);
}

const binaryPath = installedBinaryPath(resolution.platform.binaryPath);

if (!existsSync(binaryPath)) {
  console.error(
    "Driggsby native binary is not installed. Reinstall with npm scripts enabled, or rerun with npm_config_foreground_scripts=true to see installer output.",
  );
  process.exit(1);
}

spawnFileAndExit(binaryPath, process.argv.slice(2));

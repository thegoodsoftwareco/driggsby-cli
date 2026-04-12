import {
  readPackageJson,
  resolvePlatform,
  unsupportedPlatformMessage,
} from "../lib/artifacts.js";

const packageJson = readPackageJson();
const resolution = resolvePlatform(packageJson);

if (!resolution.platform) {
  console.error(unsupportedPlatformMessage(resolution));
  process.exit(1);
}

console.error(
  "Driggsby native binary is not active. Reinstall with npm scripts enabled, or rerun with npm_config_foreground_scripts=true to see installer output.",
);
process.exit(1);

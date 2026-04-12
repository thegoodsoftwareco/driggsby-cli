import {
  copyFileSync,
  cpSync,
  mkdirSync,
  readFileSync,
  rmSync,
  writeFileSync,
} from "node:fs";
import { join } from "node:path";
import { spawnFile } from "../../npm/driggsby/lib/process.js";

interface NpmPackageJson {
  driggsbyArtifacts: {
    baseUrl: string;
    checksums: Record<string, string>;
    supportedPlatforms: Record<string, { artifactName: string }>;
  };
  version: string;
}

const packageDirectory = "npm/driggsby";
const outputDirectory = "target/distrib";
const stagingDirectory = join(outputDirectory, "driggsby-npm-package");
const version = readWorkspaceVersion();

rmSync(stagingDirectory, { force: true, recursive: true });
mkdirSync(stagingDirectory, { recursive: true });
mkdirSync(join(stagingDirectory, "bin"), { recursive: true });

copyFileSync("LICENSE", join(stagingDirectory, "LICENSE"));
copyFileSync(join(packageDirectory, "README.md"), join(stagingDirectory, "README.md"));
copyNodeEntrypoint("dist/npm/driggsby/install.js", join(stagingDirectory, "install.js"));
copyNodeEntrypoint(
  "dist/npm/driggsby/bin/driggsby.js",
  join(stagingDirectory, "bin", "driggsby"),
);
cpSync("dist/npm/driggsby/lib", join(stagingDirectory, "lib"), { recursive: true });
writeFileSync(
  join(stagingDirectory, "package.json"),
  `${JSON.stringify(buildPackageJson(version), null, 2)}\n`,
);

spawnFile("npm", ["pack", stagingDirectory, "--pack-destination", outputDirectory]);

function buildPackageJson(packageVersion: string): NpmPackageJson {
  const packageJson = JSON.parse(
    readFileSync(join(packageDirectory, "package.json"), "utf8"),
  ) as NpmPackageJson;

  packageJson.version = packageVersion;
  packageJson.driggsbyArtifacts.baseUrl =
    `https://github.com/thegoodsoftwareco/driggsby-cli/releases/download/driggsby-v${packageVersion}`;
  packageJson.driggsbyArtifacts.checksums = {};

  for (const platform of Object.values(packageJson.driggsbyArtifacts.supportedPlatforms)) {
    packageJson.driggsbyArtifacts.checksums[platform.artifactName] = readSha256(
      join(outputDirectory, `${platform.artifactName}.sha256`),
    );
  }

  return packageJson;
}

function readWorkspaceVersion(): string {
  const cargoToml = readFileSync("Cargo.toml", "utf8");
  const match = /^version = "([^"]+)"$/m.exec(cargoToml);

  if (!match?.[1]) {
    throw new Error("Could not find workspace package version in Cargo.toml.");
  }

  return match[1];
}

function readSha256(path: string): string {
  const contents = readFileSync(path, "utf8").trim();
  const [hash] = contents.split(/\s+/);

  if (!hash || !/^[0-9a-f]{64}$/i.test(hash)) {
    throw new Error(`Invalid SHA-256 checksum file: ${path}`);
  }

  return hash.toLowerCase();
}

function copyNodeEntrypoint(source: string, destination: string): void {
  const contents = readFileSync(source, "utf8");
  const withShebang = contents.startsWith("#!/usr/bin/env node")
    ? contents
    : `#!/usr/bin/env node\n${contents}`;

  writeFileSync(destination, withShebang, { mode: 0o755 });
}

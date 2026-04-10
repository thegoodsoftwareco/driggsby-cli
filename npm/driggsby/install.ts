import { createHash } from "node:crypto";
import {
  chmodSync,
  createWriteStream,
  existsSync,
  mkdirSync,
  readFileSync,
  rmSync,
} from "node:fs";
import { tmpdir } from "node:os";
import { basename, dirname, join } from "node:path";
import { pipeline } from "node:stream/promises";
import { fileURLToPath } from "node:url";
import { spawnFile } from "./lib/process.js";

type ArtifactConfig = {
  artifactName: string;
  binaryPath: string;
};

type PackageJson = {
  driggsbyArtifacts: {
    baseUrl: string;
    checksums: Record<string, string>;
    supportedPlatforms: Record<string, ArtifactConfig>;
  };
};

const packageDirectory = dirname(fileURLToPath(import.meta.url));
const installDirectory = join(packageDirectory, "node_modules", ".bin_real");
const packageJson = JSON.parse(
  readFileSync(join(packageDirectory, "package.json"), "utf8"),
) as PackageJson;

try {
  await install();
} catch (error) {
  if (error instanceof Error) {
    console.error(error.message);
  } else {
    console.error(error);
  }

  process.exit(1);
}

async function install(): Promise<void> {
  const platform = resolvePlatform();
  const binaryPath = join(installDirectory, platform.binaryPath);

  if (existsSync(binaryPath)) {
    return;
  }

  rmSync(installDirectory, { force: true, recursive: true });
  mkdirSync(installDirectory, { recursive: true });

  const artifactUrl = `${packageJson.driggsbyArtifacts.baseUrl}/${platform.artifactName}`;
  const tempArtifact = join(tmpdir(), `${process.pid}-${platform.artifactName}`);

  try {
    const expectedChecksum = packageJson.driggsbyArtifacts.checksums[platform.artifactName];
    if (!expectedChecksum) {
      throw new Error(`Driggsby package is missing a checksum for ${platform.artifactName}.`);
    }

    await downloadFile(artifactUrl, tempArtifact);
    verifyChecksum(tempArtifact, expectedChecksum);
    extractTarball(tempArtifact, installDirectory);
    chmodSync(binaryPath, 0o755);
  } finally {
    rmSync(tempArtifact, { force: true });
  }
}

function resolvePlatform(): ArtifactConfig {
  const triple = targetTriple();
  const platform = packageJson.driggsbyArtifacts.supportedPlatforms[triple];

  if (!platform) {
    const supported = Object.keys(packageJson.driggsbyArtifacts.supportedPlatforms)
      .sort()
      .join(", ");
    throw new Error(
      `Driggsby does not currently publish a native binary for ${process.platform}/${process.arch}. Supported targets: ${supported}.`,
    );
  }

  return platform;
}

function targetTriple(): string {
  const architecture = mapArchitecture(process.arch);
  const operatingSystem = mapOperatingSystem(process.platform);

  return `${architecture}-${operatingSystem}`;
}

function mapArchitecture(architecture: NodeJS.Architecture): string {
  if (architecture === "arm64") {
    return "aarch64";
  }

  if (architecture === "x64") {
    return "x86_64";
  }

  return architecture;
}

function mapOperatingSystem(platform: NodeJS.Platform): string {
  if (platform === "darwin") {
    return "apple-darwin";
  }

  if (platform === "linux") {
    return "unknown-linux-gnu";
  }

  if (platform === "win32") {
    return "pc-windows-msvc";
  }

  return platform;
}

async function downloadFile(url: string, destination: string): Promise<void> {
  const response = await fetch(url);

  if (!response.ok || response.body === null) {
    throw new Error(`Could not download Driggsby binary ${url}: HTTP ${response.status}.`);
  }

  await pipeline(response.body, createWriteStream(destination));
}

function verifyChecksum(path: string, expectedChecksum: string): void {
  const actualChecksum = createHash("sha256").update(readFileSync(path)).digest("hex");

  if (actualChecksum !== expectedChecksum) {
    throw new Error(
      `Driggsby binary checksum mismatch for ${basename(path)}. Expected ${expectedChecksum}, got ${actualChecksum}.`,
    );
  }
}

function extractTarball(artifactPath: string, destination: string): void {
  spawnFile("tar", ["-xf", artifactPath, "--strip-components", "1", "-C", destination]);
}

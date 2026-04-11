import { createHash } from "node:crypto";
import {
  chmodSync,
  createWriteStream,
  existsSync,
  lstatSync,
  mkdtempSync,
  mkdirSync,
  rmSync,
} from "node:fs";
import { tmpdir } from "node:os";
import { basename, join } from "node:path";
import { Transform } from "node:stream";
import { pipeline } from "node:stream/promises";
import {
  installDirectory,
  installedBinaryPath,
  readPackageJson,
  resolvePlatform,
  unsupportedPlatformMessage,
} from "./lib/artifacts.js";
import { readCommandOutput, writeCommandOutputToFile } from "./lib/process.js";
const maxArtifactBytes = 64 * 1024 * 1024;
const packageJson = readPackageJson();

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
  const resolution = resolvePlatform(packageJson);
  const platform = resolution.platform;

  if (!platform) {
    console.warn(unsupportedPlatformMessage(resolution));
    return;
  }

  const binaryPath = installedBinaryPath(platform.binaryPath);

  if (existsSync(binaryPath)) {
    return;
  }

  rmSync(installDirectory, { force: true, recursive: true });
  mkdirSync(installDirectory, { recursive: true });

  const artifactUrl = `${packageJson.driggsbyArtifacts.baseUrl}/${platform.artifactName}`;
  const tempDirectory = mkdtempSync(join(tmpdir(), "driggsby-install-"));
  chmodSync(tempDirectory, 0o700);
  const tempArtifact = join(tempDirectory, platform.artifactName);

  try {
    const expectedChecksum = packageJson.driggsbyArtifacts.checksums[platform.artifactName];
    if (!expectedChecksum) {
      throw new Error(`Driggsby package is missing a checksum for ${platform.artifactName}.`);
    }

    await downloadAndVerifyFile(artifactUrl, tempArtifact, expectedChecksum);
    extractTarball(tempArtifact, installDirectory, platform.binaryPath);
    chmodSync(binaryPath, 0o755);
  } catch (error) {
    rmSync(installDirectory, { force: true, recursive: true });
    throw error;
  } finally {
    rmSync(tempDirectory, { force: true, recursive: true });
  }
}

async function downloadAndVerifyFile(
  url: string,
  destination: string,
  expectedChecksum: string,
): Promise<void> {
  const response = await fetch(url);

  if (!response.ok || response.body === null) {
    throw new Error(`Could not download Driggsby binary ${url}: HTTP ${response.status}.`);
  }

  const contentLength = response.headers.get("content-length");
  if (contentLength && Number(contentLength) > maxArtifactBytes) {
    throw new Error(`Driggsby binary download is unexpectedly large: ${contentLength} bytes.`);
  }

  const hash = createHash("sha256");
  let bytesRead = 0;
  const verifier = new Transform({
    transform(chunk: Buffer, _encoding, callback) {
      bytesRead += chunk.byteLength;
      if (bytesRead > maxArtifactBytes) {
        callback(new Error(`Driggsby binary download exceeded ${maxArtifactBytes} bytes.`));
        return;
      }

      hash.update(chunk);
      callback(null, chunk);
    },
  });

  await pipeline(response.body, verifier, createWriteStream(destination, { flags: "wx", mode: 0o600 }));

  const actualChecksum = hash.digest("hex");

  if (actualChecksum !== expectedChecksum) {
    throw new Error(
      `Driggsby binary checksum mismatch for ${basename(destination)}. Expected ${expectedChecksum}, got ${actualChecksum}.`,
    );
  }
}

function extractTarball(artifactPath: string, destination: string, binaryPath: string): void {
  const tarPath = trustedTarPath();
  const entry = findBinaryTarEntry(tarPath, artifactPath, binaryPath);
  const installedBinary = join(destination, binaryPath);

  writeCommandOutputToFile(tarPath, ["-xOf", artifactPath, "--", entry], installedBinary, maxArtifactBytes);

  if (!existsSync(installedBinary)) {
    throw new Error(`Driggsby archive did not contain expected binary ${binaryPath}.`);
  }

  const installedBinaryStats = lstatSync(installedBinary);
  if (!installedBinaryStats.isFile() || installedBinaryStats.isSymbolicLink()) {
    throw new Error(`Driggsby archive entry ${binaryPath} was not a regular file.`);
  }
}

function trustedTarPath(): string {
  for (const candidate of ["/usr/bin/tar", "/bin/tar"]) {
    if (existsSync(candidate)) {
      return candidate;
    }
  }

  throw new Error("Could not find the system tar executable required to unpack Driggsby.");
}

function findBinaryTarEntry(tarPath: string, artifactPath: string, binaryPath: string): string {
  const entries = readCommandOutput(tarPath, ["-tf", artifactPath])
    .split("\n")
    .map((entry) => entry.trim())
    .filter((entry) => entry.length > 0);
  const binaryEntries: string[] = [];

  for (const entry of entries) {
    const normalizedEntry = entry.replace(/\\/g, "/");
    const segments = normalizedEntry.split("/").filter((segment) => segment.length > 0);

    if (
      normalizedEntry.startsWith("/") ||
      /^[A-Za-z]:/.test(normalizedEntry) ||
      segments.includes("..") ||
      segments.some((segment) => segment.startsWith("-"))
    ) {
      throw new Error(`Driggsby archive contains unsafe path ${entry}.`);
    }

    if (segments.length === 2 && segments[1] === binaryPath) {
      binaryEntries.push(entry);
    }
  }

  if (binaryEntries.length !== 1) {
    throw new Error(`Driggsby archive did not contain exactly one ${binaryPath} binary.`);
  }

  const [binaryEntry] = binaryEntries;
  if (!binaryEntry) {
    throw new Error(`Driggsby archive did not contain expected binary ${binaryPath}.`);
  }

  return binaryEntry;
}

import { createHash } from "node:crypto";
import { execFileSync, spawn } from "node:child_process";
import {
  cpSync,
  mkdirSync,
  mkdtempSync,
  readFileSync,
  rmSync,
  writeFileSync,
} from "node:fs";
import { createServer } from "node:http";
import { tmpdir } from "node:os";
import { join } from "node:path";

type JsonPrimitive = boolean | number | string | null;
type JsonValue = JsonPrimitive | JsonValue[] | { [key: string]: JsonValue };

interface ArtifactConfig {
  checksums: Record<string, unknown>;
  supportedPlatforms: Record<string, unknown>;
}

interface ChecksumBehaviorOptions {
  artifactConfig: ArtifactConfig;
  packageJsonPath: string;
  publishDir: string;
}

export async function assertChecksumVerificationBehavior(
  options: ChecksumBehaviorOptions,
): Promise<void> {
  const originalPackageJson = readFileSync(options.packageJsonPath, "utf8");
  const platform = assertRecord(
    options.artifactConfig.supportedPlatforms[currentTargetTriple()],
    "current platform config",
  );
  const artifactName = assertString(platform.artifactName, "current platform artifactName");
  const fakeArtifact = Buffer.from("not a valid driggsby artifact tar", "utf8");
  const server = createServer((request, response) => {
    if (request.url !== `/${artifactName}`) {
      response.writeHead(404).end();
      return;
    }

    response.writeHead(200, {
      "content-length": String(fakeArtifact.byteLength),
      "content-type": "application/octet-stream",
    });
    response.end(fakeArtifact);
  });

  try {
    const port = await listenOnLocalhost(server);
    const modifiedPackageJson = JSON.parse(originalPackageJson) as JsonValue;
    if (!isRecord(modifiedPackageJson)) {
      throw new Error("package.json must be a JSON object.");
    }

    modifiedPackageJson.driggsbyArtifacts = {
      baseUrl: `http://127.0.0.1:${port}`,
      checksums: { [artifactName]: "0".repeat(64) },
      supportedPlatforms: toJsonRecord(options.artifactConfig.supportedPlatforms),
    };
    writeFileSync(options.packageJsonPath, `${JSON.stringify(modifiedPackageJson, null, 2)}\n`);

    const result = await runNodeScript(options.publishDir, "install.js", []);
    const output = `${result.stdout}${result.stderr}`;

    if (result.status === 0 || !output.includes("checksum mismatch")) {
      throw new Error("install.js did not fail closed on an artifact checksum mismatch.");
    }

    assertFallbackBinTarget(options.publishDir, "install.js replaced the fallback after a checksum mismatch.");
  } finally {
    writeFileSync(options.packageJsonPath, originalPackageJson);
    await closeServer(server);
  }
}

export async function assertNativeBinActivationBehavior(
  options: ChecksumBehaviorOptions,
): Promise<void> {
  const originalPackageJson = readFileSync(options.packageJsonPath, "utf8");
  const binTargetPath = join(options.publishDir, "bin", "driggsby");
  const originalBinTarget = readFileSync(binTargetPath);
  const platform = assertRecord(
    options.artifactConfig.supportedPlatforms[currentTargetTriple()],
    "current platform config",
  );
  const artifactName = assertString(platform.artifactName, "current platform artifactName");
  const fakeArtifact = makeFakeArtifact(artifactName);
  const server = createServer((request, response) => {
    if (request.url !== `/${artifactName}`) {
      response.writeHead(404).end();
      return;
    }

    response.writeHead(200, {
      "content-length": String(fakeArtifact.byteLength),
      "content-type": "application/octet-stream",
    });
    response.end(fakeArtifact);
  });

  try {
    const port = await listenOnLocalhost(server);
    const modifiedPackageJson = JSON.parse(originalPackageJson) as JsonValue;
    if (!isRecord(modifiedPackageJson)) {
      throw new Error("package.json must be a JSON object.");
    }

    modifiedPackageJson.driggsbyArtifacts = {
      baseUrl: `http://127.0.0.1:${port}`,
      checksums: { [artifactName]: sha256(fakeArtifact) },
      supportedPlatforms: toJsonRecord(options.artifactConfig.supportedPlatforms),
    };
    writeFileSync(options.packageJsonPath, `${JSON.stringify(modifiedPackageJson, null, 2)}\n`);

    const installResult = await runNodeScript(options.publishDir, "install.js", []);
    const installOutput = `${installResult.stdout}${installResult.stderr}`;

    if (installResult.status !== 0) {
      throw new Error(`install.js did not activate the native bin target: ${installOutput}`);
    }

    const runResult = await runCommand(join(options.publishDir, "bin", "driggsby"), ["--probe"], options.publishDir);
    if (runResult.status !== 0 || runResult.stdout.trim() !== "native-fast --probe") {
      throw new Error("installed bin target did not run the activated artifact.");
    }
  } finally {
    writeFileSync(options.packageJsonPath, originalPackageJson);
    writeFileSync(binTargetPath, originalBinTarget, { mode: 0o755 });
    await closeServer(server);
  }
}

export async function assertNpmBinLinkBehavior(
  options: ChecksumBehaviorOptions,
): Promise<void> {
  const tempDirectory = mkdtempSync(join(tmpdir(), "driggsby-npm-bin-test-"));
  const packageCopy = join(tempDirectory, "package");
  const platform = assertRecord(
    options.artifactConfig.supportedPlatforms[currentTargetTriple()],
    "current platform config",
  );
  const artifactName = assertString(platform.artifactName, "current platform artifactName");
  const fakeArtifact = makeFakeArtifact(artifactName);
  const server = createServer((request, response) => {
    if (request.url !== `/${artifactName}`) {
      response.writeHead(404).end();
      return;
    }

    response.writeHead(200, {
      "content-length": String(fakeArtifact.byteLength),
      "content-type": "application/octet-stream",
    });
    response.end(fakeArtifact);
  });

  try {
    cpSync(options.publishDir, packageCopy, { recursive: true });
    const packageJsonPath = join(packageCopy, "package.json");
    const port = await listenOnLocalhost(server);
    const modifiedPackageJson = JSON.parse(readFileSync(packageJsonPath, "utf8")) as JsonValue;
    if (!isRecord(modifiedPackageJson)) {
      throw new Error("package.json must be a JSON object.");
    }

    modifiedPackageJson.driggsbyArtifacts = {
      baseUrl: `http://127.0.0.1:${port}`,
      checksums: { [artifactName]: sha256(fakeArtifact) },
      supportedPlatforms: toJsonRecord(options.artifactConfig.supportedPlatforms),
    };
    writeFileSync(packageJsonPath, `${JSON.stringify(modifiedPackageJson, null, 2)}\n`);

    const tarballPath = packTestPackage(packageCopy, tempDirectory);
    await assertGlobalNpmInstall(tarballPath, tempDirectory);
    await assertGlobalNpmUpgrade(tarballPath, tempDirectory);
    await assertLocalNpmInstall(tarballPath, tempDirectory);
    await assertNpmExec(tarballPath, tempDirectory);
  } finally {
    await closeServer(server);
    rmSync(tempDirectory, { force: true, recursive: true });
  }
}

export async function assertUnsupportedPlatformBehavior(
  options: ChecksumBehaviorOptions,
): Promise<void> {
  const originalPackageJson = readFileSync(options.packageJsonPath, "utf8");

  try {
    const modifiedPackageJson = JSON.parse(originalPackageJson) as JsonValue;
    if (!isRecord(modifiedPackageJson)) {
      throw new Error("package.json must be a JSON object.");
    }

    modifiedPackageJson.driggsbyArtifacts = {
      baseUrl: "https://example.invalid/driggsby",
      checksums: {},
      supportedPlatforms: {
        "unsupported-test-target": {
          artifactName: "driggsby-unsupported-test-target.tar.xz",
          binaryPath: "driggsby",
        },
      },
    };
    writeFileSync(options.packageJsonPath, `${JSON.stringify(modifiedPackageJson, null, 2)}\n`);
    rmSync(join(options.publishDir, "node_modules"), { force: true, recursive: true });

    const installResult = await runNodeScript(options.publishDir, "install.js", []);
    const installOutput = `${installResult.stdout}${installResult.stderr}`;
    if (installResult.status !== 0 || !installOutput.includes("does not currently publish")) {
      throw new Error("install.js did not complete with a visible unsupported-platform warning.");
    }

    assertFallbackBinTarget(options.publishDir, "install.js replaced the fallback for an unsupported platform.");

    const runResult = await runNodeScript(options.publishDir, "bin/driggsby", ["--version"]);
    const runOutput = `${runResult.stdout}${runResult.stderr}`;
    if (runResult.status === 0 || !runOutput.includes("does not currently publish")) {
      throw new Error("bin/driggsby did not surface the unsupported-platform error.");
    }
  } finally {
    writeFileSync(options.packageJsonPath, originalPackageJson);
  }
}

function makeFakeArtifact(artifactName: string): Buffer {
  const tempDirectory = mkdtempSync(join(tmpdir(), "driggsby-artifact-test-"));
  const packageDirectory = join(tempDirectory, "driggsby-test");
  const binaryPath = join(packageDirectory, "driggsby");
  const artifactPath = join(tempDirectory, artifactName);

  try {
    mkdirSync(packageDirectory, { recursive: true });
    writeFileSync(binaryPath, "#!/bin/sh\necho native-fast \"$@\"\n", { mode: 0o700 });
    execFileSync("tar", ["-cJf", artifactPath, "-C", tempDirectory, "driggsby-test"], {
      stdio: "ignore",
    });

    return readFileSync(artifactPath);
  } finally {
    rmSync(tempDirectory, { force: true, recursive: true });
  }
}

function sha256(contents: Buffer): string {
  return createHash("sha256").update(contents).digest("hex");
}

function assertFallbackBinTarget(publishDir: string, message: string): void {
  const binTarget = readFileSync(join(publishDir, "bin", "driggsby"), "utf8");

  if (!binTarget.includes("Driggsby native binary is not active")) {
    throw new Error(message);
  }
}

function packTestPackage(packageDirectory: string, outputDirectory: string): string {
  const packOutput = execFileSync(
    "npm",
    ["pack", packageDirectory, "--pack-destination", outputDirectory],
    { encoding: "utf8", stdio: ["ignore", "pipe", "ignore"] },
  ).trim();
  const [tarballName] = packOutput.split("\n").slice(-1);

  if (!tarballName) {
    throw new Error("npm pack did not report an output tarball.");
  }

  return join(outputDirectory, tarballName);
}

async function assertGlobalNpmInstall(tarballPath: string, tempDirectory: string): Promise<void> {
  const prefix = join(tempDirectory, "global-prefix");
  const installResult = await runCommand("npm", ["install", "-g", tarballPath, "--prefix", prefix], tempDirectory);
  if (installResult.status !== 0) {
    throw new Error(`global npm install did not complete: ${installResult.stderr}`);
  }

  const runResult = await runCommand(join(prefix, "bin", "driggsby"), ["--probe"], tempDirectory);
  if (runResult.status !== 0 || runResult.stdout.trim() !== "native-fast --probe") {
    throw new Error("global npm install did not activate the native bin link.");
  }
}

async function assertGlobalNpmUpgrade(tarballPath: string, tempDirectory: string): Promise<void> {
  const prefix = join(tempDirectory, "upgrade-prefix");
  const oldTarballPath = packOldShapePackage(tempDirectory);
  let installResult = await runCommand("npm", ["install", "-g", oldTarballPath, "--prefix", prefix], tempDirectory);
  if (installResult.status !== 0) {
    throw new Error(`old-shape global npm install did not complete: ${installResult.stderr}`);
  }

  installResult = await runCommand("npm", ["install", "-g", tarballPath, "--prefix", prefix], tempDirectory);
  if (installResult.status !== 0) {
    throw new Error(`global npm upgrade did not complete: ${installResult.stderr}`);
  }

  const runResult = await runCommand(join(prefix, "bin", "driggsby"), ["--probe"], tempDirectory);
  if (runResult.status !== 0 || runResult.stdout.trim() !== "native-fast --probe") {
    throw new Error("global npm upgrade did not activate the native bin link.");
  }
}

function packOldShapePackage(tempDirectory: string): string {
  const oldPackage = join(tempDirectory, "old-package");
  mkdirSync(oldPackage, { recursive: true });
  writeFileSync(
    join(oldPackage, "package.json"),
    '{"name":"driggsby","version":"0.1.20","bin":{"driggsby":"run-driggsby.js"}}\n',
  );
  writeFileSync(join(oldPackage, "run-driggsby.js"), "#!/bin/sh\necho old-shim\n", { mode: 0o755 });
  return packTestPackage(oldPackage, tempDirectory);
}

async function assertLocalNpmInstall(tarballPath: string, tempDirectory: string): Promise<void> {
  const projectDirectory = join(tempDirectory, "local-project");
  mkdirSync(projectDirectory, { recursive: true });
  const installResult = await runCommand("npm", ["install", tarballPath], projectDirectory);
  if (installResult.status !== 0) {
    throw new Error(`local npm install did not complete: ${installResult.stderr}`);
  }

  const runResult = await runCommand(
    join(projectDirectory, "node_modules", ".bin", "driggsby"),
    ["--probe"],
    projectDirectory,
  );
  if (runResult.status !== 0 || runResult.stdout.trim() !== "native-fast --probe") {
    throw new Error("local npm install did not activate the native bin link.");
  }
}

async function assertNpmExec(tarballPath: string, tempDirectory: string): Promise<void> {
  const execResult = await runCommand(
    "npm",
    ["exec", "--yes", `--package=${tarballPath}`, "--", "driggsby", "--probe"],
    tempDirectory,
  );
  if (execResult.status !== 0 || !execResult.stdout.includes("native-fast --probe")) {
    throw new Error(`npm exec did not activate the native bin link: ${execResult.stderr}`);
  }
}

async function listenOnLocalhost(server: ReturnType<typeof createServer>): Promise<number> {
  await new Promise<void>((resolveListen, rejectListen) => {
    server.once("error", rejectListen);
    server.listen(0, "127.0.0.1", () => {
      server.off("error", rejectListen);
      resolveListen();
    });
  });

  const address = server.address();
  if (address === null || typeof address === "string") {
    throw new Error("Could not determine local checksum-test server port.");
  }

  return (address).port;
}

async function runNodeScript(publishDir: string, scriptName: string, args: string[]): Promise<{
  status: number | null;
  stderr: string;
  stdout: string;
}> {
  return await runCommand(process.execPath, [scriptName, ...args], publishDir);
}

async function runCommand(command: string, args: string[], cwd: string): Promise<{
  status: number | null;
  stderr: string;
  stdout: string;
}> {
  return await new Promise((resolveRun, rejectRun) => {
    const child = spawn(command, args, {
      cwd,
      stdio: ["ignore", "pipe", "pipe"],
    });
    let stderr = "";
    let stdout = "";
    const timeout = setTimeout(() => {
      child.kill("SIGTERM");
      rejectRun(new Error(`${command} behavior check timed out.`));
    }, 10_000);

    child.stdout.setEncoding("utf8");
    child.stderr.setEncoding("utf8");
    child.stdout.on("data", (chunk: string) => {
      stdout += chunk;
    });
    child.stderr.on("data", (chunk: string) => {
      stderr += chunk;
    });
    child.on("error", (error) => {
      clearTimeout(timeout);
      rejectRun(error);
    });
    child.on("exit", (status) => {
      clearTimeout(timeout);
      resolveRun({ status, stderr, stdout });
    });
  });
}

async function closeServer(server: ReturnType<typeof createServer>): Promise<void> {
  if (!server.listening) {
    return;
  }

  server.closeAllConnections();

  await new Promise<void>((resolveClose, rejectClose) => {
    server.close((error) => {
      if (error) {
        rejectClose(error);
        return;
      }

      resolveClose();
    });
  });
}

function currentTargetTriple(): string {
  const architecture = process.arch === "arm64"
    ? "aarch64"
    : process.arch === "x64"
      ? "x86_64"
      : process.arch;
  const operatingSystem = process.platform === "darwin"
    ? "apple-darwin"
    : process.platform === "linux"
      ? "unknown-linux-gnu"
      : process.platform === "win32"
        ? "pc-windows-msvc"
        : process.platform;

  return `${architecture}-${operatingSystem}`;
}

function assertRecord(value: unknown, label: string): Record<string, unknown> {
  if (!isRecord(value)) {
    throw new Error(`${label} must be an object.`);
  }

  return value;
}

function assertString(value: unknown, label: string): string {
  if (typeof value !== "string" || value.length === 0) {
    throw new Error(`${label} must be a non-empty string.`);
  }

  return value;
}

function isRecord(value: unknown): value is Record<string, unknown> {
  return typeof value === "object" && value !== null && !Array.isArray(value);
}

function toJsonRecord(record: Record<string, unknown>): Record<string, JsonValue> {
  const rendered: Record<string, JsonValue> = {};

  for (const [key, value] of Object.entries(record)) {
    if (!isJsonValue(value)) {
      throw new Error(`${key} must be JSON-serializable.`);
    }

    rendered[key] = value;
  }

  return rendered;
}

function isJsonValue(value: unknown): value is JsonValue {
  if (
    value === null ||
    typeof value === "boolean" ||
    typeof value === "number" ||
    typeof value === "string"
  ) {
    return true;
  }

  if (Array.isArray(value)) {
    return value.every(isJsonValue);
  }

  if (!isRecord(value)) {
    return false;
  }

  return Object.values(value).every(isJsonValue);
}

import { spawn } from "node:child_process";
import {
  existsSync,
  readFileSync,
  rmSync,
  writeFileSync,
} from "node:fs";
import { createServer } from "node:http";
import { join } from "node:path";

type JsonValue =
  | boolean
  | number
  | string
  | null
  | JsonValue[]
  | { [key: string]: JsonValue };

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

    if (existsSync(join(options.publishDir, "node_modules", ".bin_real", "driggsby"))) {
      throw new Error("install.js extracted a binary after an artifact checksum mismatch.");
    }
  } finally {
    writeFileSync(options.packageJsonPath, originalPackageJson);
    await closeServer(server);
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

    if (existsSync(join(options.publishDir, "node_modules", ".bin_real", "driggsby"))) {
      throw new Error("install.js extracted a binary for an unsupported platform.");
    }

    const runResult = await runNodeScript(options.publishDir, "run-driggsby.js", ["--version"]);
    const runOutput = `${runResult.stdout}${runResult.stderr}`;
    if (runResult.status === 0 || !runOutput.includes("does not currently publish")) {
      throw new Error("run-driggsby.js did not surface the unsupported-platform error.");
    }
  } finally {
    writeFileSync(options.packageJsonPath, originalPackageJson);
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
  return await new Promise((resolveRun, rejectRun) => {
    const child = spawn(process.execPath, [scriptName, ...args], {
      cwd: publishDir,
      stdio: ["ignore", "pipe", "pipe"],
    });
    let stderr = "";
    let stdout = "";
    const timeout = setTimeout(() => {
      child.kill("SIGTERM");
      rejectRun(new Error(`${scriptName} behavior check timed out.`));
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

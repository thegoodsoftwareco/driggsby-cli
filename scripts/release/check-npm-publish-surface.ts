import { execFileSync } from "node:child_process";
import {
  existsSync,
  mkdtempSync,
  readdirSync,
  readFileSync,
  rmSync,
  statSync,
} from "node:fs";
import { tmpdir } from "node:os";
import { basename, dirname, extname, join, relative, resolve } from "node:path";
import { fileURLToPath } from "node:url";

type JsonValue =
  | boolean
  | number
  | string
  | null
  | JsonValue[]
  | { [key: string]: JsonValue };

type PackageJson = {
  artifactDownloadUrls?: unknown;
  bin?: unknown;
  glibcMinimum?: unknown;
  license?: unknown;
  name?: unknown;
  repository?: unknown;
  scripts?: unknown;
  supportedPlatforms?: unknown;
  version?: unknown;
};

type PlatformConfig = {
  artifactName?: unknown;
  bins?: unknown;
  zipExt?: unknown;
};

const expectedPackageName = "driggsby";
const expectedRepository = "thegoodsoftwareco/driggsby-cli";
const expectedPlatforms = new Set([
  "aarch64-apple-darwin",
  "x86_64-apple-darwin",
  "x86_64-unknown-linux-gnu",
]);
const forbiddenNativeExtensions = new Set([".dylib", ".exe", ".so", ".xz", ".zip"]);
const forbiddenNativeFileNames = new Set(["driggsby"]);
const requiredJsEntrypoints = [
  "binary-install.js",
  "binary.js",
  "install.js",
  "run-driggsby.js",
];

const thisFilePath = fileURLToPath(import.meta.url);
const rootDir = findRepoRoot(dirname(thisFilePath));
const configPath = join(
  rootDir,
  "scripts",
  "release",
  "npm-publish-surface.secretlintrc.json",
);
const tarballInput = process.argv[2];

if (!tarballInput) {
  throw new Error(
    "Usage: `node dist/scripts/release/check-npm-publish-surface.js <tarball>`",
  );
}

const tarballPath = resolve(process.cwd(), tarballInput);
const tempDir = mkdtempSync(join(tmpdir(), "driggsby-pack-scan-"));

try {
  execFileSync("tar", ["-xzf", tarballPath, "-C", tempDir], {
    stdio: "inherit",
  });

  const publishDir = join(tempDir, "package");
  assertDirectoryExists(publishDir, "The packed package did not contain package/.");

  const allFiles = listFiles(publishDir);
  const relativeFiles = allFiles.map((filePath) => relative(publishDir, filePath));

  assertPackageContract(publishDir, relativeFiles);
  assertGeneratedJavaScriptParses(publishDir);
  secretScanTextFiles(publishDir, allFiles);

  process.stdout.write(
    `NPM package surface check passed for ${relative(rootDir, tarballPath)}.\n`,
  );
} finally {
  rmSync(tempDir, { force: true, recursive: true });
}

function assertPackageContract(publishDir: string, relativeFiles: string[]): void {
  const packageJson = readPackageJson(join(publishDir, "package.json"));
  const version = assertString(packageJson.version, "package.json version");
  const artifactDownloadUrls = assertStringArray(
    packageJson.artifactDownloadUrls,
    "package.json artifactDownloadUrls",
  );
  const supportedPlatforms = assertRecord(
    packageJson.supportedPlatforms,
    "package.json supportedPlatforms",
  );

  assertEqual(packageJson.name, expectedPackageName, "package.json name");
  assertEqual(packageJson.license, "Apache-2.0", "package.json license");
  assertEqual(
    packageJson.repository,
    `https://github.com/${expectedRepository}`,
    "package.json repository",
  );
  assertEqual(version, version.trim(), "package.json version must be trimmed");
  assertMatchingVersion(version);
  assertExpectedBin(packageJson.bin);
  assertExpectedScripts(packageJson.scripts);
  assertExpectedGlibcMinimum(packageJson.glibcMinimum);
  assertExpectedArtifactDownloadUrls(artifactDownloadUrls, version);
  assertExpectedPlatforms(supportedPlatforms);
  assertNoEmbeddedNativeArtifacts(relativeFiles);
}

function findRepoRoot(startDirectory: string): string {
  let currentDirectory = resolve(startDirectory);

  while (true) {
    const candidatePackageJson = join(currentDirectory, "package.json");
    const candidateConfig = join(
      currentDirectory,
      "scripts",
      "release",
      "npm-publish-surface.secretlintrc.json",
    );

    if (existsSync(candidatePackageJson) && existsSync(candidateConfig)) {
      return currentDirectory;
    }

    const parentDirectory = dirname(currentDirectory);
    if (parentDirectory === currentDirectory) {
      throw new Error("Could not find repository root for npm package surface check.");
    }

    currentDirectory = parentDirectory;
  }
}

function readPackageJson(path: string): PackageJson {
  const parsed = JSON.parse(readFileSync(path, "utf8")) as JsonValue;

  if (!isRecord(parsed)) {
    throw new Error("package.json must be a JSON object.");
  }

  return parsed;
}

function assertMatchingVersion(version: string): void {
  if (!/^\d+\.\d+\.\d+([.-][0-9A-Za-z.-]+)?$/.test(version)) {
    throw new Error(`package.json version is not a supported SemVer: ${version}`);
  }
}

function assertExpectedBin(value: unknown): void {
  const bin = assertRecord(value, "package.json bin");
  assertEqual(bin.driggsby, "run-driggsby.js", "package.json bin.driggsby");
}

function assertExpectedScripts(value: unknown): void {
  const scripts = assertRecord(value, "package.json scripts");
  assertEqual(scripts.postinstall, "node ./install.js", "package.json scripts.postinstall");
}

function assertExpectedGlibcMinimum(value: unknown): void {
  const glibcMinimum = assertRecord(value, "package.json glibcMinimum");
  assertEqual(glibcMinimum.major, 2, "package.json glibcMinimum.major");
  if (typeof glibcMinimum.series !== "number") {
    throw new Error("package.json glibcMinimum.series must be a number.");
  }
}

function assertExpectedArtifactDownloadUrls(urls: string[], version: string): void {
  const expectedUrl = `https://github.com/${expectedRepository}/releases/download/driggsby-v${version}`;

  if (urls.length !== 1) {
    throw new Error(`Expected exactly one artifactDownloadUrls entry, got ${urls.length}.`);
  }

  const primaryUrl = urls[0];
  if (primaryUrl === undefined) {
    throw new Error("package.json artifactDownloadUrls[0] is missing.");
  }

  assertEqual(primaryUrl, expectedUrl, "package.json artifactDownloadUrls[0]");

  const parsedUrl = new URL(primaryUrl);
  assertEqual(parsedUrl.protocol, "https:", "artifact URL protocol");
  assertEqual(parsedUrl.hostname, "github.com", "artifact URL host");
  if (parsedUrl.pathname.includes("/thegoodsoftwareco/driggsby/releases/")) {
    throw new Error("artifact URL points at the private driggsby repository.");
  }
}

function assertExpectedPlatforms(platforms: Record<string, unknown>): void {
  const actualPlatforms = new Set(Object.keys(platforms));

  for (const expectedPlatform of expectedPlatforms) {
    if (!actualPlatforms.has(expectedPlatform)) {
      throw new Error(`Missing supported platform ${expectedPlatform}.`);
    }
  }

  for (const actualPlatform of actualPlatforms) {
    if (!expectedPlatforms.has(actualPlatform)) {
      throw new Error(`Unexpected supported platform ${actualPlatform}.`);
    }

    const platform = assertRecord(platforms[actualPlatform], `platform ${actualPlatform}`);
    assertPlatformConfig(actualPlatform, platform);
  }
}

function assertPlatformConfig(platformName: string, platform: PlatformConfig): void {
  const artifactName = assertString(platform.artifactName, `${platformName}.artifactName`);
  const bins = assertRecord(platform.bins, `${platformName}.bins`);

  assertEqual(artifactName, `driggsby-${platformName}.tar.xz`, `${platformName}.artifactName`);
  assertEqual(platform.zipExt, ".tar.xz", `${platformName}.zipExt`);
  assertEqual(bins.driggsby, "driggsby", `${platformName}.bins.driggsby`);
}

function assertNoEmbeddedNativeArtifacts(relativeFiles: string[]): void {
  for (const filePath of relativeFiles) {
    const name = basename(filePath);
    const extension = extname(filePath);

    if (forbiddenNativeExtensions.has(extension) || forbiddenNativeFileNames.has(name)) {
      throw new Error(`NPM package must stay thin; found native artifact ${filePath}.`);
    }
  }
}

function assertGeneratedJavaScriptParses(publishDir: string): void {
  for (const relativePath of requiredJsEntrypoints) {
    const path = join(publishDir, relativePath);
    assertFileExists(path, `Missing generated JS entrypoint ${relativePath}.`);
    execFileSync("node", ["--check", path], { stdio: "inherit" });
  }
}

function secretScanTextFiles(publishDir: string, allFiles: string[]): void {
  const textFiles = allFiles
    .filter((filePath) => looksTextual(filePath))
    .map((filePath) => relative(publishDir, filePath));

  if (textFiles.length === 0) {
    throw new Error("The packed package did not contain any text files to scan.");
  }

  execFileSync(
    "npx",
    [
      "--yes",
      "--package",
      "secretlint@11.4.1",
      "--package",
      "@secretlint/secretlint-rule-preset-recommend@11.4.1",
      "secretlint",
      "--secretlintrc",
      configPath,
      ...textFiles,
    ],
    {
      cwd: publishDir,
      stdio: "inherit",
    },
  );
}

function listFiles(directory: string): string[] {
  const files: string[] = [];

  for (const entry of readdirSync(directory, { withFileTypes: true })) {
    const absolutePath = join(directory, entry.name);

    if (entry.isDirectory()) {
      files.push(...listFiles(absolutePath));
      continue;
    }

    files.push(absolutePath);
  }

  return files;
}

function looksTextual(filePath: string): boolean {
  const sample = readFileSync(filePath);
  const sampleSize = Math.min(sample.length, 8000);

  for (let index = 0; index < sampleSize; index += 1) {
    if (sample[index] === 0) {
      return false;
    }
  }

  return true;
}

function assertFileExists(path: string, message: string): void {
  if (!existsSync(path) || !statSync(path).isFile()) {
    throw new Error(message);
  }
}

function assertDirectoryExists(path: string, message: string): void {
  if (!existsSync(path) || !statSync(path).isDirectory()) {
    throw new Error(message);
  }
}

function assertString(value: unknown, label: string): string {
  if (typeof value !== "string" || value.length === 0) {
    throw new Error(`${label} must be a non-empty string.`);
  }

  return value;
}

function assertStringArray(value: unknown, label: string): string[] {
  if (!Array.isArray(value) || !value.every((entry) => typeof entry === "string")) {
    throw new Error(`${label} must be an array of strings.`);
  }

  return value;
}

function assertRecord(value: unknown, label: string): Record<string, unknown> {
  if (!isRecord(value)) {
    throw new Error(`${label} must be an object.`);
  }

  return value;
}

function isRecord(value: unknown): value is Record<string, unknown> {
  return typeof value === "object" && value !== null && !Array.isArray(value);
}

function assertEqual(actual: unknown, expected: unknown, label: string): void {
  if (actual !== expected) {
    throw new Error(`${label}: expected ${String(expected)}, got ${String(actual)}.`);
  }
}

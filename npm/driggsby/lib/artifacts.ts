import { readFileSync } from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";

export interface ArtifactConfig {
  artifactName: string;
  binaryPath: string;
}

export interface PackageJson {
  driggsbyArtifacts: {
    baseUrl: string;
    checksums: Record<string, string>;
    supportedPlatforms: Record<string, ArtifactConfig>;
  };
}

export interface PlatformResolution {
  platform: ArtifactConfig | undefined;
  supportedTargets: string[];
  triple: string;
}

export const packageDirectory = dirname(dirname(fileURLToPath(import.meta.url)));
export const installDirectory = join(packageDirectory, "node_modules", ".bin_real");

export function readPackageJson(): PackageJson {
  return JSON.parse(readFileSync(join(packageDirectory, "package.json"), "utf8")) as PackageJson;
}

export function installedBinaryPath(binaryPath: string): string {
  return join(installDirectory, binaryPath);
}

export function resolvePlatform(packageJson: PackageJson): PlatformResolution {
  const triple = targetTriple();

  return {
    platform: packageJson.driggsbyArtifacts.supportedPlatforms[triple],
    supportedTargets: Object.keys(packageJson.driggsbyArtifacts.supportedPlatforms).sort(),
    triple,
  };
}

export function unsupportedPlatformMessage(resolution: PlatformResolution): string {
  const supported = resolution.supportedTargets.length > 0
    ? resolution.supportedTargets.join(", ")
    : "none";

  return `Driggsby does not currently publish a native binary for ${process.platform}/${process.arch} (${resolution.triple}). Supported targets: ${supported}.`;
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

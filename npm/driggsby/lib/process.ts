import { spawnSync } from "node:child_process";
import { writeFileSync } from "node:fs";

export interface SpawnResult {
  status: number;
}

export function runFile(command: string, args: string[]): SpawnResult {
  const result = spawnSync(command, args, {
    cwd: process.cwd(),
    stdio: "inherit",
  });

  if (result.error) {
    throw result.error;
  }

  return { status: result.status ?? 1 };
}

export function spawnFile(command: string, args: string[]): void {
  const result = runFile(command, args);

  if (result.status !== 0) {
    throw new Error(`${command} exited with status ${result.status}.`);
  }
}

export function readCommandOutput(command: string, args: string[]): string {
  const result = spawnSync(command, args, {
    cwd: process.cwd(),
    encoding: "utf8",
    maxBuffer: 1024 * 1024,
  });

  if (result.error) {
    throw result.error;
  }

  if (result.status !== 0) {
    const stderr = result.stderr.trim();
    const detail = stderr.length > 0 ? `: ${stderr}` : "";
    throw new Error(`${command} exited with status ${result.status ?? 1}${detail}.`);
  }

  return result.stdout;
}

export function writeCommandOutputToFile(
  command: string,
  args: string[],
  destination: string,
  maxBytes: number,
): void {
  const result = spawnSync(command, args, {
    cwd: process.cwd(),
    encoding: "buffer",
    maxBuffer: maxBytes,
  });

  if (result.error) {
    throw result.error;
  }

  if (result.status !== 0) {
    const stderr = result.stderr.toString("utf8").trim();
    const detail = stderr.length > 0 ? `: ${stderr}` : "";
    throw new Error(`${command} exited with status ${result.status ?? 1}${detail}.`);
  }

  writeFileSync(destination, result.stdout, { flag: "wx", mode: 0o700 });
}

export function spawnFileAndExit(command: string, args: string[]): never {
  try {
    const result = runFile(command, args);
    process.exit(result.status);
  } catch (error) {
    if (error instanceof Error) {
      console.error(error.message);
    } else {
      console.error(error);
    }

    process.exit(1);
  }
}

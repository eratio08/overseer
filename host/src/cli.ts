/**
 * CLI bridge - spawns `os` binary and parses JSON output
 *
 * Configuration is passed via constructor, not environment variables.
 * This allows the Rust CLI to set paths correctly when spawning.
 */
import { spawn } from "node:child_process";
import { CliError, CliTimeoutError } from "./types.js";

const CLI_TIMEOUT_MS = 30_000;

export interface CliConfig {
  /** Path to the os binary */
  cliPath: string;
  /** Working directory for CLI commands */
  cwd: string;
}

// Global config, set by main entry point
let config: CliConfig = {
  cliPath: "os",
  cwd: process.cwd(),
};

/**
 * Configure the CLI bridge
 */
export function configureCli(newConfig: CliConfig): void {
  config = newConfig;
}

/**
 * Get current CLI config
 */
export function getCliConfig(): CliConfig {
  return config;
}

/**
 * Execute os CLI command with --json flag
 */
export async function callCli(args: string[]): Promise<unknown> {
  return new Promise((resolve, reject) => {
    const proc = spawn(config.cliPath, [...args, "--json"], {
      cwd: config.cwd,
      stdio: ["ignore", "pipe", "pipe"],
    });

    const timeout = setTimeout(() => {
      proc.kill("SIGTERM");
      reject(new CliTimeoutError());
    }, CLI_TIMEOUT_MS);

    let stdout = "";
    let stderr = "";

    proc.stdout.on("data", (chunk: Buffer) => {
      stdout += chunk.toString();
    });

    proc.stderr.on("data", (chunk: Buffer) => {
      stderr += chunk.toString();
    });

    proc.on("error", (err) => {
      clearTimeout(timeout);
      reject(new CliError(`Failed to spawn os: ${err.message}`, -1, ""));
    });

    proc.on("close", (code) => {
      clearTimeout(timeout);

      if (code !== 0) {
        const message = stderr.trim() || `os exited with code ${code}`;
        reject(new CliError(message, code ?? -1, stderr));
        return;
      }

      try {
        const result: unknown = JSON.parse(stdout);
        resolve(result);
      } catch (err) {
        reject(
          new CliError(
            `Invalid JSON from os: ${err instanceof Error ? err.message : String(err)}`,
            0,
            stdout
          )
        );
      }
    });
  });
}

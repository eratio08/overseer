#!/usr/bin/env node

import { execSync } from "node:child_process";
import { existsSync, readFileSync } from "node:fs";

function isMusl() {
  if (process.platform !== "linux") return false;
  if (existsSync("/etc/alpine-release")) return true;

  try {
    if (readFileSync("/proc/self/maps", "utf8").includes("musl")) return true;
  } catch {}

  try {
    return execSync("ldd --version 2>&1", { encoding: "utf8" })
      .toLowerCase()
      .includes("musl");
  } catch (err) {
    const stderr = String(err.stderr || err.message || "");
    return stderr.toLowerCase().includes("musl");
  }
}

const pkg = JSON.parse(readFileSync("npm/overseer/package.json", "utf8"));
const platforms = JSON.parse(readFileSync("npm/scripts/platforms.json", "utf8"));

const cpuArch = process.arch === "arm64" ? "arm64" : "x64";
const libc = isMusl() ? "-musl" : "";
const platformKey = `${process.platform}-${cpuArch}${libc}`;
const platformConfig = platforms[platformKey];

if (!platformConfig) {
  console.error(`Unsupported platform: ${platformKey}`);
  process.exit(1);
}

const platformPackageName = `${pkg.name}-${platformKey}`;
const npmPrefix = execSync("npm prefix -g", { encoding: "utf8" }).trim();

const values = {
  version: pkg.version,
  mainPackageName: pkg.name,
  platformKey,
  rustTarget: platformConfig.rust_target,
  platformPackageName,
  mainTarball: `${pkg.name.slice(1).replace("/", "-")}-${pkg.version}.tgz`,
  platformTarball: `${platformPackageName.slice(1).replace("/", "-")}-${pkg.version}.tgz`,
  npmPrefix,
  installedOs: `${npmPrefix}/bin/os`,
};

const field = process.argv[2];

if (!field || !(field in values)) {
  console.error(`Usage: node npm/scripts/local-package-meta.mjs <${Object.keys(values).join("|")}>`);
  process.exit(1);
}

process.stdout.write(values[field]);

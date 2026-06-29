import { cp, mkdir } from "node:fs/promises";
import { execFileSync } from "node:child_process";
import { homedir } from "node:os";
import { join } from "node:path";

const cargo = join(homedir(), ".cargo", "bin", "cargo");
const manifest = join("helper", "Cargo.toml");
const target = "aarch64-apple-darwin";
const targetDir = join("helper", "target");

execFileSync(
  cargo,
  [
    "build",
    "--manifest-path",
    manifest,
    "--release",
    "--target",
    target,
    "--target-dir",
    targetDir,
  ],
  { stdio: "inherit" },
);

const binaries = join("src-tauri", "binaries");
await mkdir(binaries, { recursive: true });
await cp(
  join(targetDir, target, "release", "daily-email-cleanout-helper"),
  join(binaries, `daily-email-cleanout-helper-${target}`),
);

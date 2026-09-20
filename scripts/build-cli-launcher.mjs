import { spawnSync } from "node:child_process";
import { copyFileSync, mkdirSync } from "node:fs";
import path from "node:path";

const rustc = process.platform === "win32" ? "rustc.exe" : "rustc";
const hostResult = spawnSync(rustc, ["--print", "host-tuple"], {
  encoding: "utf8",
  shell: false,
});
if (hostResult.status !== 0) {
  process.stderr.write(hostResult.stderr ?? "Failed to resolve Rust host tuple.\n");
  process.exit(hostResult.status ?? 1);
}

const host = hostResult.stdout.trim();
const target = process.env.TAURI_ENV_TARGET_TRIPLE || host;
if (!target.includes('windows')) {
  console.log('macOS/Linux use the marklite main binary for CLI; no launcher is distributed.');
  process.exit(0);
}
const isDebug = process.env.TAURI_ENV_DEBUG === "true";
const targetRoot = path.resolve(process.env.CARGO_TARGET_DIR || "src-tauri/target");
const outputDirectory = path.join(
  targetRoot,
  ...(target === host ? [] : [target]),
  isDebug ? "debug" : "release",
);
const extension = target.includes("windows") ? ".exe" : "";
mkdirSync(outputDirectory, { recursive: true });
const launcherPath = path.join(outputDirectory, `marklite-cli${extension}`);
const args = [
  "src-tauri/launcher/marklite-cli.rs",
  "--edition=2021",
  "-C",
  `opt-level=${isDebug ? "0" : "z"}`,
  "-C",
  "panic=abort",
  "-C",
  "strip=symbols",
  "-o",
  launcherPath,
];
if (target === "x86_64-pc-windows-msvc") {
  args.push("-C", "target-feature=+crt-static");
}
if (target !== host) {
  args.push("--target", target);
}

const result = spawnSync(rustc, args, { stdio: "inherit", shell: false });
if (result.error) {
  console.error(`Failed to start rustc for marklite-cli: ${result.error.message}`);
  process.exit(1);
}
if (result.status !== 0) process.exit(result.status ?? 1);

// Tauri resolves externalBin using the target triple, while direct CLI callers
// use the unsuffixed launcher beside the main executable.
const sidecarDirectory = path.resolve("src-tauri/binaries");
mkdirSync(sidecarDirectory, { recursive: true });
copyFileSync(launcherPath, path.join(sidecarDirectory, `marklite-cli-${target}${extension}`));

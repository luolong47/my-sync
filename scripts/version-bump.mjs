import { readFile, writeFile } from "node:fs/promises";
import path from "node:path";
import process from "node:process";

const SUPPORTED_RELEASE_TYPES = new Set(["patch", "minor"]);

function parseVersion(version) {
  const match = /^(\d+)\.(\d+)\.(\d+)$/.exec(version);
  if (!match) {
    throw new Error(`不支持的版本号格式: ${version}，仅支持 x.y.z`);
  }

  return {
    major: Number(match[1]),
    minor: Number(match[2]),
    patch: Number(match[3]),
  };
}

function formatVersion({ major, minor, patch }) {
  return `${major}.${minor}.${patch}`;
}

function bumpVersion(currentVersion, releaseType) {
  const version = parseVersion(currentVersion);

  if (releaseType === "patch") {
    version.patch += 1;
    return formatVersion(version);
  }

  if (releaseType === "minor") {
    version.minor += 1;
    version.patch = 0;
    return formatVersion(version);
  }

  throw new Error(`不支持的版本递增类型: ${releaseType}`);
}

async function readJson(filePath) {
  const content = await readFile(filePath, "utf8");
  return {
    content,
    data: JSON.parse(content),
  };
}

async function writeJson(filePath, data) {
  await writeFile(filePath, `${JSON.stringify(data, null, 2)}\n`, "utf8");
}

async function loadTargets(repoRoot) {
  const targets = [];
  const packageJsonPath = path.join(repoRoot, "package.json");
  const packageJson = await readJson(packageJsonPath);

  if (!packageJson.data.version) {
    throw new Error("package.json 缺少 version 字段");
  }

  targets.push({
    kind: "package.json",
    filePath: packageJsonPath,
    version: packageJson.data.version,
    async write(nextVersion) {
      packageJson.data.version = nextVersion;
      await writeJson(packageJsonPath, packageJson.data);
    },
  });

  const cargoTomlPath = path.join(repoRoot, "src-tauri", "Cargo.toml");
  try {
    const cargoTomlContent = await readFile(cargoTomlPath, "utf8");
    const cargoMatch = /(\[package\][\s\S]*?^version\s*=\s*")(\d+\.\d+\.\d+)(")/m.exec(
      cargoTomlContent,
    );

    if (cargoMatch) {
      targets.push({
        kind: "Cargo.toml",
        filePath: cargoTomlPath,
        version: cargoMatch[2],
        async write(nextVersion) {
          const nextContent = cargoTomlContent.replace(
            /(\[package\][\s\S]*?^version\s*=\s*")(\d+\.\d+\.\d+)(")/m,
            `$1${nextVersion}$3`,
          );
          await writeFile(cargoTomlPath, nextContent, "utf8");
        },
      });
    }
  } catch (error) {
    if (error.code !== "ENOENT") {
      throw error;
    }
  }

  const tauriConfPath = path.join(repoRoot, "src-tauri", "tauri.conf.json");
  try {
    const tauriConf = await readJson(tauriConfPath);
    if (tauriConf.data.version) {
      targets.push({
        kind: "tauri.conf.json",
        filePath: tauriConfPath,
        version: tauriConf.data.version,
        async write(nextVersion) {
          tauriConf.data.version = nextVersion;
          await writeJson(tauriConfPath, tauriConf.data);
        },
      });
    }
  } catch (error) {
    if (error.code !== "ENOENT") {
      throw error;
    }
  }

  return targets;
}

function ensureVersionsAreAligned(targets) {
  const versionMap = new Map();

  for (const target of targets) {
    if (!versionMap.has(target.version)) {
      versionMap.set(target.version, []);
    }

    versionMap.get(target.version).push(target);
  }

  if (versionMap.size <= 1) {
    return [...versionMap.keys()][0];
  }

  const details = [...versionMap.entries()]
    .map(([version, items]) => {
      const files = items.map((item) => path.relative(process.cwd(), item.filePath)).join("、");
      return `${version}: ${files}`;
    })
    .join("\n");

  throw new Error(`检测到版本号不一致，已停止更新：\n${details}`);
}

async function main() {
  const releaseType = process.argv[2] ?? "patch";

  if (!SUPPORTED_RELEASE_TYPES.has(releaseType)) {
    throw new Error("用法: node scripts/version-bump.mjs <patch|minor>");
  }

  const repoRoot = process.cwd();
  const targets = await loadTargets(repoRoot);
  const currentVersion = ensureVersionsAreAligned(targets);
  const nextVersion = bumpVersion(currentVersion, releaseType);

  for (const target of targets) {
    await target.write(nextVersion);
  }

  console.log(`版本号已从 ${currentVersion} 更新为 ${nextVersion}`);
  for (const target of targets) {
    console.log(`已同步 ${path.relative(repoRoot, target.filePath)}`);
  }
}

main().catch((error) => {
  console.error(error instanceof Error ? error.message : String(error));
  process.exitCode = 1;
});

import { readdir, readFile } from "node:fs/promises";
import path from "node:path";
import process from "node:process";

const MAX_LINES = 500;
const RUST_SOURCE_DIR = path.join(process.cwd(), "src");

async function collectRustFiles(dirPath) {
  const entries = await readdir(dirPath, { withFileTypes: true });
  const files = [];

  for (const entry of entries) {
    const fullPath = path.join(dirPath, entry.name);

    if (entry.isDirectory()) {
      files.push(...await collectRustFiles(fullPath));
      continue;
    }

    if (entry.isFile() && entry.name.endsWith(".rs")) {
      files.push(fullPath);
    }
  }

  return files;
}

function countEffectiveLines(content) {
  const lines = content.split(/\r?\n/);
  let count = 0;
  let inBlockComment = false;

  for (const rawLine of lines) {
    const line = rawLine.trim();

    if (!line) {
      continue;
    }

    if (inBlockComment) {
      if (line.includes("*/")) {
        const afterBlock = line.slice(line.indexOf("*/") + 2).trim();
        inBlockComment = false;

        if (!afterBlock || afterBlock.startsWith("//")) {
          continue;
        }
      } else {
        continue;
      }
    }

    if (line.startsWith("//")) {
      continue;
    }

    if (line.startsWith("/*")) {
      if (!line.includes("*/")) {
        inBlockComment = true;
        continue;
      }

      const afterBlock = line.slice(line.indexOf("*/") + 2).trim();
      if (!afterBlock || afterBlock.startsWith("//")) {
        continue;
      }
    }

    if (line === "*" || line.startsWith("* ")) {
      continue;
    }

    count += 1;
  }

  return count;
}

async function main() {
  const rustFiles = await collectRustFiles(RUST_SOURCE_DIR);
  const violations = [];

  for (const filePath of rustFiles) {
    const content = await readFile(filePath, "utf8");
    const effectiveLines = countEffectiveLines(content);

    if (effectiveLines > MAX_LINES) {
      violations.push({
        filePath,
        effectiveLines,
      });
    }
  }

  if (violations.length === 0) {
    console.log(`Rust 文件行数检查通过，单文件有效行数未超过 ${MAX_LINES} 行`);
    return;
  }

  console.error(`Rust 文件行数检查失败，以下文件有效行数超过 ${MAX_LINES} 行：`);
  for (const violation of violations) {
    console.error(
      `- ${path.relative(process.cwd(), violation.filePath)}: ${violation.effectiveLines} 行`,
    );
  }

  process.exitCode = 1;
}

main().catch((error) => {
  console.error(error instanceof Error ? error.message : String(error));
  process.exitCode = 1;
});

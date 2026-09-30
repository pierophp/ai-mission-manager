import fs from "node:fs";
import path from "node:path";
import process from "node:process";
import { fileURLToPath } from "node:url";

const FORBIDDEN = [
  ["removed goal command", /\/goal\b/i],
  ["removed Cursor team kit", /cursor-team-kit/i],
  ["removed cloud execution field", /environment:\s*["']cloud["']/i],
  ["removed Cursor home path", /~\/.cursor\//i],
];
const MARKDOWN_LINK = /\]\(([^)]+)\)/g;
const CODE_PATH = /`([^`<>\n]+\.(?:md|mjs|ts|tsx|js|sh|json|ya?ml))`/g;
const SOURCE_EXTENSIONS = new Set([
  ".md",
  ".mjs",
  ".ts",
  ".tsx",
  ".js",
  ".sh",
  ".json",
  ".yaml",
  ".yml",
]);

function textFiles(root) {
  const files = [];
  const visit = (directory) => {
    for (const entry of fs.readdirSync(directory, { withFileTypes: true })) {
      const entryPath = path.join(directory, entry.name);
      if (entry.isDirectory()) visit(entryPath);
      else if (SOURCE_EXTENSIONS.has(path.extname(entry.name).toLowerCase())) {
        files.push(entryPath);
      }
    }
  };
  visit(root);
  return files;
}

function linkPath(value) {
  const trimmed = value.trim().replace(/^<|>$/g, "").split(/[?#]/, 1)[0];
  if (
    !trimmed ||
    trimmed === "url" ||
    /^[a-z][a-z0-9+.-]*:/i.test(trimmed) ||
    trimmed.startsWith("#")
  ) {
    return null;
  }
  return trimmed;
}

function referencedPaths(sourcePath, content, treeRoot) {
  const references = [];
  for (const match of content.matchAll(MARKDOWN_LINK)) {
    const target = linkPath(match[1]);
    if (target) references.push({ target, relativeToSource: true });
  }
  for (const match of content.matchAll(CODE_PATH)) {
    const target = match[1].trim();
    if (target.includes("<") || target.includes(">")) continue;
    references.push({ target, relativeToSource: !target.startsWith("skills/") });
  }
  return references.map(({ target, relativeToSource }) => {
    const sourceCandidate = path.resolve(path.dirname(sourcePath), target);
    const rootCandidate = path.resolve(treeRoot, target);
    const candidates = relativeToSource
      ? [sourceCandidate, rootCandidate, path.resolve(treeRoot, "skills/poteto-mode", target)]
      : [rootCandidate];
    return { target, candidates };
  });
}

export function checkTree(treeRoot) {
  const root = path.resolve(treeRoot);
  const failures = [];
  if (!fs.existsSync(root) || !fs.statSync(root).isDirectory()) {
    return [`vendor tree does not exist: ${root}`];
  }

  const files = textFiles(root);
  for (const file of files) {
    if (path.basename(file) === "PATCHES.md") continue;
    const content = fs.readFileSync(file, "utf8");
    for (const [description, pattern] of FORBIDDEN) {
      pattern.lastIndex = 0;
      if (pattern.test(content)) {
        failures.push(`${path.relative(root, file)}: contains ${description}`);
      }
    }
  }

  const modeRoot = path.join(root, "skills/poteto-mode");
  if (!fs.existsSync(modeRoot)) {
    failures.push("skills/poteto-mode: required tree is missing");
  } else {
    for (const file of textFiles(modeRoot).filter((candidate) => candidate.endsWith(".md"))) {
      const content = fs.readFileSync(file, "utf8");
      for (const { target, candidates } of referencedPaths(file, content, root)) {
        if (!candidates.some((candidate) => fs.existsSync(candidate))) {
          failures.push(
            `${path.relative(root, file)}: referenced path does not exist: ${target}`
          );
        }
      }
    }
  }

  return failures;
}

const defaultRoot = path.resolve(
  path.dirname(fileURLToPath(import.meta.url)),
  "../agents/pstack"
);
const invokedPath = process.argv[1] ? path.resolve(process.argv[1]) : null;
if (invokedPath === fileURLToPath(import.meta.url)) {
  const failures = checkTree(process.argv[2] ?? defaultRoot);
  if (failures.length > 0) {
    console.error(failures.join("\n"));
    process.exitCode = 1;
  } else {
    console.log("pstack vendor checks passed");
  }
}

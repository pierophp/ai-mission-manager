import assert from "node:assert/strict";
import fs from "node:fs";
import os from "node:os";
import path from "node:path";
import test from "node:test";
import { checkTree } from "./check-pstack-vendor.mjs";

function fixture() {
  const root = fs.mkdtempSync(path.join(os.tmpdir(), "pstack-vendor-"));
  fs.mkdirSync(path.join(root, "skills/poteto-mode"), { recursive: true });
  fs.writeFileSync(path.join(root, "skills/poteto-mode/SKILL.md"), "# Mode\n");
  fs.writeFileSync(
    path.join(root, "skills/poteto-mode/playbook.md"),
    "[mode](./SKILL.md)\n"
  );
  return root;
}

test("accepts a tree with safe references", () => {
  const root = fixture();
  try {
    assert.deepEqual(checkTree(root), []);
  } finally {
    fs.rmSync(root, { recursive: true, force: true });
  }
});

test("rejects removed Cursor-only facilities", () => {
  const forbiddenReferences = [
    ["/goal", /removed goal command/],
    ["cursor-team-kit", /removed Cursor team kit/],
    ['environment: "cloud"', /removed cloud execution field/],
    ["~/.cursor/rules/file", /removed Cursor home path/],
  ];
  for (const [reference, expected] of forbiddenReferences) {
    const root = fixture();
    try {
      fs.writeFileSync(path.join(root, "README.md"), `${reference}\n`);
      assert.match(checkTree(root).join("\n"), expected);
    } finally {
      fs.rmSync(root, { recursive: true, force: true });
    }
  }
});

test("rejects paths missing from poteto-mode", () => {
  const root = fixture();
  try {
    fs.writeFileSync(
      path.join(root, "skills/poteto-mode/playbook.md"),
      "[missing](./missing.md)\n"
    );
    assert.match(checkTree(root).join("\n"), /referenced path does not exist/);
  } finally {
    fs.rmSync(root, { recursive: true, force: true });
  }
});

test("rejects missing paths written in poteto-mode instructions", () => {
  const root = fixture();
  try {
    fs.writeFileSync(
      path.join(root, "skills/poteto-mode/playbook.md"),
      "Read `skills/poteto-mode/missing.md`.\n"
    );
    assert.match(checkTree(root).join("\n"), /referenced path does not exist/);
  } finally {
    fs.rmSync(root, { recursive: true, force: true });
  }
});

test("checks command arguments and ignores bare runtime file names", () => {
  const root = fixture();
  try {
    fs.mkdirSync(path.join(root, "skills/poteto-mode/scripts"), { recursive: true });
    fs.writeFileSync(path.join(root, "skills/poteto-mode/scripts/orch.ts"), "");
    fs.writeFileSync(
      path.join(root, "skills/poteto-mode/playbook.md"),
      "Use `bun scripts/orch.ts`. Keep `status.md` and `frontier.json` in the store.\n"
    );
    assert.deepEqual(checkTree(root), []);

    fs.writeFileSync(
      path.join(root, "skills/poteto-mode/playbook.md"),
      "Use `bun scripts/missing.ts`.\n"
    );
    assert.match(checkTree(root).join("\n"), /referenced path does not exist: scripts\/missing\.ts/);
  } finally {
    fs.rmSync(root, { recursive: true, force: true });
  }
});

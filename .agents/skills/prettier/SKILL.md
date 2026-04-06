---
name: prettier
description: "Workflow for running Prettier on files that were created or modified during a session. Ensures consistent formatting before changes are staged or committed."
---

### Instructions

```xml
<description>
This skill formats files that were created or modified during the current
session using Prettier. It should be run automatically after all edits for
a task are complete, before staging or committing the changes.
</description>
```

### Workflow

**Follow these steps:**

1. Collect the list of files that were **created or modified** during the current task. Use `git diff --name-only` plus `git ls-files --others --exclude-standard` to find unstaged changes.
2. Filter the list to only files that Prettier supports (`.ts`, `.tsx`, `.js`, `.jsx`, `.css`, `.json`, `.md`, etc.).
3. Run Prettier on those files using the project's local installation:

```bash
yarn prettier --write <file1> <file2> ...
```

4. If files were changed by Prettier, note that formatting was applied. Do **not** treat formatting changes as separate commits — they should be included in the same commit as the code changes.
5. If `yarn prettier --write` fails, fall back to:

```bash
npx prettier --write <file1> <file2> ...
```

### Supported Extensions

```xml
<extensions>
  <ext>.ts</ext>
  <ext>.tsx</ext>
  <ext>.js</ext>
  <ext>.jsx</ext>
  <ext>.mjs</ext>
  <ext>.cjs</ext>
  <ext>.css</ext>
  <ext>.json</ext>
  <ext>.md</ext>
  <ext>.yaml</ext>
  <ext>.yml</ext>
</extensions>
```

### Notes

- Never run Prettier on generated files (`generated/`, `dist/`, `node_modules/`).
- Prettier config is located at `prettier.config.js` in the project root — it is picked up automatically.
- Tailwind class ordering is handled by `prettier-plugin-tailwindcss`; rely on it rather than manually sorting classes.

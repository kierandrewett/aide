---
name: git-push
description: "Prompt and workflow for pushing commits to the remote repository once a meaningful batch of commits has accumulated. Prevents excessive small pushes while keeping the remote branch reasonably up to date."
---

### Instructions

```xml
<description>
This skill pushes local commits to the configured remote branch after a
substantial number of commits have accumulated. The goal is to avoid
pushing every single commit while still keeping the remote repository
synchronised at sensible intervals.
</description>
```

### Workflow

**Follow these steps:**

1. Run `git status` to confirm the working tree is clean.
2. Run `git log origin/$(git rev-parse --abbrev-ref HEAD)..HEAD --oneline` to list commits that have not yet been pushed.
3. Count the number of commits ahead of the remote branch.
4. If the number of commits is **greater than or equal to the push threshold**, proceed with the push.
5. If the threshold has **not** been reached, do nothing and continue committing normally.
6. When the threshold is reached, Copilot will automatically execute the push command in the integrated terminal.

### Push Threshold

```xml
<threshold>
<commits>5</commits>
<description>
Push once 5 or more commits exist locally that have not yet been pushed
to the remote branch. This helps group related changes into a single push.
</description>
</threshold>
```

### Push Command

```xml
<push-command>
<cmd>git push origin $(git rev-parse --abbrev-ref HEAD)</cmd>
</push-command>
```

### Safety Checks

```xml
<safety>
<working-tree>
Ensure the working tree is clean before pushing.
</working-tree>

<remote>
Confirm that a remote named "origin" exists.
</remote>

<diverged-branch>
If the branch has diverged from origin, perform a pull with rebase before pushing.
</diverged-branch>
</safety>
```

### Examples

```xml
<examples>
<example>
Local commits ahead of origin: 2
Action: Do not push yet.
</example>

<example>
Local commits ahead of origin: 5
Action: Push commits to origin.
</example>

<example>
Local commits ahead of origin: 8
Action: Push commits to origin.
</example>
</examples>
```

### Final Step

```xml
<final-step>
<cmd>git push origin $(git rev-parse --abbrev-ref HEAD)</cmd>
<note>
Executed automatically once the commit threshold is met.
</note>
</final-step>
```

### Behaviour Summary

```xml
<behaviour>
<rule>
Do not push after every commit.
</rule>

<rule>
Push when at least the defined commit threshold has accumulated.
</rule>

<rule>
Always verify the branch and working tree state before pushing.
</rule>
</behaviour>
```

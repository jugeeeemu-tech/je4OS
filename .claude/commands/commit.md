---
description: Generate git-cliff compatible commit message (Conventional Commits)
allowed-tools: Bash(git diff:*), Bash(git status:*), Bash(git log:*), Bash(git add:*), Bash(git commit:*), Bash(git reset:*)
---

## Context

- Staged changes: !`git diff --cached --stat`
- Unstaged changes: !`git diff --stat`
- Current status: !`git status --short`
- Recent commits: !`git log --oneline -5`

## Atomic Commit Principle

**One commit = One logical change**

Before creating commits, analyze ALL changes and group them by logical unit:
- Each feature, fix, or refactor should be a separate commit
- Related file changes that serve the same purpose go together
- Unrelated changes must be split into separate commits

## Analysis Process

1. **List all changed files** and understand what each change does
2. **Group changes** by logical purpose:
   - Group A: files related to feature X
   - Group B: files related to bug fix Y
   - Group C: files related to refactor Z
3. **Plan commits** in logical order (dependencies first)
4. **Present the plan** to the user before executing

## Commit Plan Format

Present your analysis like this:

```
## Commit Plan

### Commit 1: feat(auth): add OAuth2 login
Files:
- src/auth/oauth.ts (new)
- src/auth/index.ts (modified)
- src/types/auth.ts (modified)

### Commit 2: fix(dashboard): correct chart rendering
Files:
- src/components/Chart.tsx (modified)

### Commit 3: chore: update dependencies
Files:
- package.json (modified)
- pnpm-lock.yaml (modified)
```

## Execution Process

After user approval:
1. `git reset HEAD` (unstage all if needed)
2. For each commit:
   - `git add <specific files>`
   - `git commit -m "<message>"`
3. Show final `git log --oneline` to confirm

## Conventional Commits Format

```
<type>(<scope>): <subject>

<body>

<footer>
```

### Type (Required)

| Type | Description |
|------|-------------|
| `feat` | New feature |
| `fix` | Bug fix |
| `docs` | Documentation only |
| `style` | Code style (formatting, semicolons, etc.) |
| `refactor` | Code refactoring (no feature/fix) |
| `perf` | Performance improvement |
| `test` | Adding/updating tests |
| `build` | Build system or dependencies |
| `ci` | CI configuration |
| `chore` | Other maintenance tasks |
| `revert` | Revert a previous commit |

### Scope (Optional)

Affected module or component in parentheses. Examples:
- `feat(auth):`
- `fix(dashboard):`
- `refactor(api):`

### Subject (Required)

- Imperative mood ("add" not "added")
- Lowercase first letter
- No period at end
- Max 50 characters

### Body (Optional)

- Explain **what** and **why**, not how
- Wrap at 72 characters
- Blank line between subject and body

### Footer (Optional)

- `Closes #123` - Close an issue
- `Refs #456` - Reference an issue
- `BREAKING CHANGE:` - Breaking change description

### Breaking Changes

For breaking changes, add exclamation mark (!) after type/scope:

    feat(api)!: change authentication method

    BREAKING CHANGE: JWT tokens now required for all endpoints

## Your Task

1. **Analyze** all staged and unstaged changes
2. **Group** changes into logical atomic commits
3. **Present** the commit plan with files and messages
4. **Wait** for user approval
5. **Execute** commits one by one after approval

If changes are already well-grouped (single logical change), proceed with one commit.

$ARGUMENTS

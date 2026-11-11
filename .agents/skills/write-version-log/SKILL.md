---
name: write-version-log
description: Write or update the version log in documents/version_logs/ from the commits made since the last Git tag, describing every change from the user's perspective. Use when the user asks to update or write the version log, prepare release notes or a changelog, wrap up a version for release, or asks what changed since the last version — even casually ("把日志更新一下", "写下个版本的日志", "准备发版").
---

# Version logs

Version logs live in `documents/version_logs/`, one markdown file per version (e.g. `v0.0.8.md`). Writing one is a translation job: commits say how the code changed; the log says what the user experiences as changed.

## Collect the changes

1. Find the last version tag and list everything since it:

   ```bash
   git tag --sort=-creatordate | head -1
   git log --oneline <last-tag>..HEAD
   git diff --stat <last-tag>..HEAD
   ```

2. Commit messages are often terse, so open the diffs behind them — the log must be based on what actually changed, not what the message claims. Group related commits into a single entry; don't narrate every commit.
3. If a log file for the target version already exists, treat it as planning notes, not a finished log: keep the items the commits confirm, drop the ones that didn't happen, and add what the commits delivered beyond the plan.

## Write from the user's perspective

- Lead with the outcome the user sees: the bug symptom that disappears, the feature that now works, the thing that got faster or easier. The internal cause — mechanisms, modules, concurrency, data structures — is not the log's business unless it changes how the user uses the product.
- Rephrase, don't translate literally: a commit that fixes an internal flaw is written as the normal behavior the user regains.
- Refactorings and CI/build changes with no user-visible effect stay out, unless the user asks to include them.

Match the previous log's language, structure and tone — in this repo currently Chinese, `# vX.Y.Z` title with `## 功能` and `## 修复` sections. Read the most recent log before writing.

## Pick the target version

If an unreleased log file already exists, write into it. Otherwise pick the next version number from the scope of the changes and confirm it with the user before creating the file.

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

- Lead with the outcome the user sees: the bug symptom that disappears, the feature that now works, the thing that got faster or easier.
- One entry per capability. All the commits behind a single feature merge into one sentence — what the user can now do, never the variants, options or sub-improvements it ships with.
- Omit how it works internally — deployment methods, container networking, tokens and credentials, protocols, relays — no matter how hard it was to build. The same applies to fixes: write the normal behavior the user regains, never the mechanism that was broken.
- Refactorings and CI/build changes with no user-visible effect stay out, unless the user asks to include them.

## Calibrate against the house standard

Entries are coarse: one sentence per feature, at the granularity a user would describe it to another user. The standard shape, from this repo's own releases:

> Plugin支持安装XXX，YYY，安装完成的Plugin附带配套的管理界面、登录界面。

If a draft entry explains how the feature is built or deployed, it is over-detailed: cut it back to this shape — the capability gained, what it is called, what ships with it.

Match the previous log's language, structure and tone — in this repo currently Chinese, `# vX.Y.Z` title with `## 功能` and `## 修复` sections. Read the most recent log before writing.

## Pick the target version

If an unreleased log file already exists, write into it. Otherwise pick the next version number from the scope of the changes and confirm it with the user before creating the file.

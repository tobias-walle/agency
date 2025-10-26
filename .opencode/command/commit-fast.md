---
description: Commit the changes into the current git branch
agent: build
---

Commit the current changes following the rules described in the AGENTS.md.

Commit very fast:
- DO NOT read any more files
- DO NOT run any commands to read the git history
- ONLY run a single command aka `git add --all && git commit -m "[message following the conventions]"`

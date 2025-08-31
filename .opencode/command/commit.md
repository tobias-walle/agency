---
description: Commit the changes into the current git branch
agent: build
---

<context>
<recent-logs>

!`git log --oneline -15`

</recent-logs>

<status>

!`git status --porcelain=v1 -uall`

</status>

<unstaged-commits>

!`git diff --patch --submodule`

</unstaged-commits>

<staged-commits>

!`git diff --staged --patch --submodule`

</staged-commits>

</context>

Commit the current changes.

- Follow Conventional Commits
- Keep most commits in a single line. Only use the body if there are unexpected changes in the commit.

- Summarize the changes into a single sentence, starting with a lowercase verb.
- The sentence should cover why the changes were made.
- Avoid semicolons in the message and keep the title shorter than 80 chars.
- You might add a body for additional explanations, but this should be the exception.
- You can use the footer for references (like related PDRs or ADRs)
- You might want to create multiple commits if the changes are not related.

ONLY answer with one or more tool calls in the form of `git add [relevant files] && git commit -m "[message]"`

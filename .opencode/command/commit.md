---
description: Commit the changes into the current git branch
agent: build
---

Commit the current changes.

- Follow Conventional Commits
- Keep most commits in a single line. Only use the body if there are unexpected changes in the commit.

Here is the relevant context:

Recent logs:
!`git log --oneline -15`

Status:
!`git status --porcelain=v1 -uall`

Unstaged commits:
!`git diff --patch --submodule`

Staged commits:
!`git diff --staged --patch --submodule`

- Think about the reasons why these changes were made.
- Summarize the changes into a single sentence, starting with a lowercase verb.
- Avoid semicolons in the message and keep the title shorter than 80 chars.
- You might add a body for additional explanations, but this should be the exception.
- You can use the footer for references (like related PDRs or ADRs)
- You might want to create multiple commits if the changes are not related.
- Directly run `git add [relevant files] && git commit -m '[message]'` without asking the user for permission or getting more context

You MUST commit in your first answer!
Do NOT perform additional tool calls and just work with the context given above.

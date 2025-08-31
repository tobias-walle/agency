---
description: Commit the changes into the current git branch
agent: build
---

## Conventional Commits — Summary

The Conventional Commits specification is a lightweight convention on top of commit messages. It provides an easy set of rules for creating an explicit commit history; which makes it easier to write automated tools on top of. This convention dovetails with [SemVer](http://semver.org), by describing the features, fixes, and breaking changes made in commit messages.

The commit message should be structured as follows:

---

```
<type>[optional scope]: <description>

[optional body]

[optional footer(s)]
```

---

The commit contains the following structural elements, to communicate intent to the consumers of your library:

1. **fix:** a commit of the _type_ `fix` patches a bug in your codebase (this correlates with [`PATCH`](http://semver.org/#summary) in Semantic Versioning).
2. **feat:** a commit of the _type_ `feat` introduces a new feature to the codebase (this correlates with [`MINOR`](http://semver.org/#summary) in Semantic Versioning).
3. **BREAKING CHANGE:** a commit that has a footer `BREAKING CHANGE:`, or appends a `!` after the type/scope, introduces a breaking API change (correlating with [`MAJOR`](http://semver.org/#summary) in Semantic Versioning). A BREAKING CHANGE can be part of commits of any _type_.
4. _types_ other than `fix:` and `feat:` are allowed, for example (based on the Angular convention) recommends `build:`, `chore:`, `ci:`, `docs:`, `style:`, `refactor:`, `perf:`, `test:`, and others.
5. _footers_ other than `BREAKING CHANGE: <description>` may be provided and follow a convention similar to [git trailer format](https://git-scm.com/docs/git-interpret-trailers).

Additional types are not mandated by the Conventional Commits specification, and have no implicit effect in Semantic Versioning (unless they include a BREAKING CHANGE). A scope may be provided to a commit’s type, to provide additional contextual information and is contained within parenthesis, e.g., `feat(parser): add ability to parse arrays`.

## Task

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

<workflow>

You MUST commit in your first answer!
Do NOT perform additional tool calls and just work with the context given above.

1. Analyse the changes. What was added, what was removed?
2. Think about the reasons why these changes were made.
3. Summarize the changes into a single sentence, starting with a lowercase verb.
   Avoid semicolons in the message and keep the title shorter than 80 chars.
   You might add a body for additional explanations, but this should be the exception.
   You can use the footer for references (like related PDRs or ADRs)
4. Run `git add --all && git commit -m '[message]'`.
   The user will need to confirm the message and might give you feedback.

</workflow>

$ARGUMENTS

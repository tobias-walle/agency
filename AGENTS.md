<!-- OPENSPEC:START -->

# OpenSpec Instructions

These instructions are for AI assistants working in this project.

Always open `@/openspec/AGENTS.md` when the request:

- Mentions planning or proposals (words like proposal, spec, change, plan)
- Introduces new capabilities, breaking changes, architecture shifts, or big performance/security work
- Sounds ambiguous and you need the authoritative spec before coding

Use `@/openspec/AGENTS.md` to learn:

- How to create and apply change proposals
- Spec format and conventions
- Project structure and guidelines

Keep this managed block so 'openspec update' can refresh the instructions.

<!-- OPENSPEC:END -->

# Committing

- Use conventional commits e.g.:
  - `docs: add change proposal for ...`
  - `feat: implement ...`
  - `fix: fix ...`
- You MUST add the files and the create the commit in the same command for easy review e.g.:
  - `git add fileA.rs fileB.rs && git commit -m "feat: ..."`

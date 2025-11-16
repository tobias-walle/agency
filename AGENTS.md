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

# Code Style

- You SHALL keep the code linear (avoid nesting) and functions to a managable size.
- If this is not given, you MUST do the following:
  - Detect duplicated code and extract it to seperate functions
  - Detect strong nesting and create functions to reduce it
  - Use language feature to reduce nesting

# Rules

- You MUST run `just check` regulary to detect compile errors
- You MUST run `just test && just fix` after every phase and fix all errors and warnings
- If you remove code, you MUST NEVER replace it with useless comments (Like `// removed ...`, `// deleted ...`, etc.). If you find comments like this always delete them.

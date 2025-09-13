---
description: Save the plan created in the current session
agent: autoaccept
---

1. Save the plan we created in the current session in `./docs/plans/`.
   Give the plan a new incremental ID (last existing ID + 1) and a slug.
2. After saving the plan, commit the changes.
   - Only commit the plan. Do not read any other files or look at the git diff.
   - Always commit with the message `docs: add PLN-[ID] [short summary <80 chars]`

## Git Status

`git status`:
!`git status`

## Existing Plans

`ls ./docs/plans`:
!`ls ./docs/plans`

## Plan Format

Structure your plan into the following sections (replace placeholders in `[]`). Add a new line between each section:

- `# PLN-[ID]: [title]`
- `Date: [iso-timestamp without time]`
- `[short sentence what this plan is about]`
- `## Goals`
- `[goals (as a bullet point list)]`
- `## Non Goals`
- `[non-goals (as a bullet point list)]`
- `## Current Behavior`
- `[how does the system currently work (based on your research). Make sure to directly reference relevant files and code snippets.]`
- `## Solution`
- `[how will the behavior be changed to solve the problem (in bullet points). Stay high level and focus on architecture and avoid verbose Implementation details.]`
- `## Detailed Plan`
- `HINT: Update checkboxes during the implementation`
- `[Numbered, Step by step plan on how to implement the solution. Mention relevant files and code changes in relatively high detail. Make sure the order makes sense. Keep Testing and TDD in mind and always start with tests. Add empty mardown checkboxes '[ ]' before each step for later update.]`
- `## Notes`
- `HINT: Update this section during the implementation with relevant changes to the plan, problems that arised or other noteworthy things.`
- `[Notes for future readers of the plan.]`

Strictly follow the format. Don't read old plans for the format, as the format changed over time.

# Claude Code Instructions

## Purpose
When working in this repository, proactively evaluate documentation and architecture clarity before making changes.

---

## 1. Architecture Diagram Check

- Look for an existing architecture diagram in the repository.
    - Common locations:
        - `/docs`
        - `/architecture`
        - root directory
    - Common formats:
        - `.drawio`
        - `.dio`
        - exported `.png` / `.svg`

### If NO diagram is found:
- Ask the user:

> "I don’t see an architecture diagram in this repository. Would you like me to analyze the codebase and generate a draw.io diagram?"

### If a diagram IS found:
- Optionally ask:

> "I found an architecture diagram. Would you like me to review whether it is up to date with the current codebase?"

---

## 2. README Review

- Locate the `README.md` file

### Evaluate whether it is:
- Accurate with current code structure
- Reflective of current architecture
- Includes setup/run instructions
- Includes key components/services
- Not outdated or misleading

### If README appears outdated or incomplete:
Ask the user:

> "The README may be outdated or incomplete. Would you like me to review and update it based on the current codebase?"

---

## 3. Behavior Guidelines

- Do NOT automatically generate or modify:
    - architecture diagrams
    - README content

- ALWAYS ask for confirmation first

- When proposing updates:
    - Summarize what you plan to change
    - Highlight gaps or inconsistencies
    - Keep suggestions concise and actionable

---

## 4. Optional Enhancements (if user agrees)

If the user approves diagram creation:
- Prefer draw.io-compatible output
- Include:
    - services/components
    - data flow
    - external dependencies
    - infrastructure (if applicable)

If the user approves README updates:
- Ensure:
    - clear project overview
    - setup instructions
    - architecture summary
    - usage examples (if applicable)

---

## 5. General Principle

Prioritize **understanding and documentation quality** before implementing new code changes.
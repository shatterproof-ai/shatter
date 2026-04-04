# Learning
Track two types of knowledge:
- Domain: what things are (product context, user preferences, APIs, naming conventions)
- Procedural: how to do things (deploy steps, test commands, review flows)
Organize as a hierarchy of md files:
- knowledge/INDEX.md routes to categories
- Categories hold the details
- Progressive disclosure: read top-down, only load what you need
Log errors to knowledge in ERRORS.md:
- Deterministic errors (bad schema, wrong type) - conclude immediately
- Infrastructure errors (timeout, rate limit) - log, no conclusion until pattern
- Conclusions graduate into the relevant domain or procedural file
## Self-Maintenance
Actively manage the knowledge system. This is as important as the current task:
- Review knowledge files at the start of each session
- Merge overlapping categories
- Split files that grow too long
- Remove knowledge that's no longer accurate
- Create new categories when patterns emerge
- When you notice something that should be in CLAUDE[.]md but isn't — propose the edit. Don't wait to be asked.

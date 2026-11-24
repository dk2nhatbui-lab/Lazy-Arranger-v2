@AGENTS.md

Multiple agents rotate on this repo (including remote/cloud branches). At the start of a session, always run `git fetch origin`, check `git status` / `git log -n 5 --all --decorate`, pull `origin/main` (`git pull --ff-only`) if behind, and call `handoff_read` (project: `"Lazy Arranger v2"`) before making edits.

# Agent Instructions

## Tmux Agent Prompting

When you send a prompt or steering message to a tmux-backed agent session:

- inject the prompt text
- then explicitly submit it with `Cmd+m`

Do not assume plain text injection or `Enter` alone is sufficient.

If the prompt is visible in the pane but the agent does not begin working, treat it as unsubmitted until `Cmd+m` has been sent.

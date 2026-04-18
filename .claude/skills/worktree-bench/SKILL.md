---
name: worktree-bench
description: Alias for wb - Implement a VRPTW optimization technique in a git worktree, benchmark it, and report the score delta back
---

# Worktree Bench (Alias)

This is an alias for the `wb` skill.

## Usage

Use `/worktree-bench` or `/wb` -- both invoke the same skill.

## Instructions

When this skill is invoked, immediately use the Skill tool to run `wb` with any arguments passed through.

If a coordinator dispatches `/worktree-bench` or `/wb` to a tmux-backed agent, the coordinator must send the prompt text and then send `Cmd+m` to submit it.

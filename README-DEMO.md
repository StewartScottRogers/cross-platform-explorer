# Agent Swarm Demo

Hello, Stewart! Welcome to this demonstration of agent swarm collaboration.

## What is an Agent Swarm?

An **agent swarm** is a multi-agent system where specialized AI agents collaborate to complete complex tasks:

- **A coordinator agent** receives the overall goal, breaks it down into discrete subtasks, and creates a plan for how they fit together.

- **Builder agents** are spawned by the coordinator, each receiving one focused task to complete (e.g., "implement the login form," "write the database migration," "add tests for the API endpoint") — they work independently and return their results.

- **All agents share the same working directory**, so one agent's file changes are immediately visible to the next, enabling a production-line workflow where each specialist builds on the previous agent's work.

The swarm pattern turns a big, multi-step project into parallel or sequential focused work, with the coordinator ensuring coherence across all the pieces.

# 0007: Operations are tools; recipes are TOML step lists with typed bindings

Date: 2026-10-28. Status: accepted.

## Context

The same operation must be callable from the shell, from a recipe, from the lab and from an
agent over MCP, with one definition of its inputs, outputs and parameters. A use case is a
sequence of operations with data flowing between them, and the sequence must be reviewable,
reproducible and shippable as a fixture.

## Decision

Every op has one **manifest** (name, typed inputs, typed outputs, parameters with defaults and
bounds) and writes one **envelope** per invocation. It is a subcommand and an MCP tool with the
same arguments. A **recipe** is a TOML list of steps; each step has an id, typed inputs bound
to earlier steps' outputs or to the recipe's inputs, and typed outputs. Binding is checked
before anything runs: a step that asks for a `Mask` cannot be given an `Image`. rascal ships one
recipe per v1 use case (`rect`, `seam`, `remove`, `repair`, `repair-area`, `protect`), and the
literal shipped files are acceptance fixtures.

## Alternatives

- **Code-only pipelines.** Fast to write, impossible to ship, review or run from an agent
  without a code change.
- **A general workflow engine.** Far more than six recipes need; a typed list is enough and is
  checkable by eye.

## Consequences

- Adding an op is adding a manifest, a pure function and a thin `Op` impl; every surface gains
  it for free.
- The registry validates every manifest and every envelope against the schemas at start and
  in tests, so drift fails loudly.

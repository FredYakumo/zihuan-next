# Instructions

## Development Guidelines

- **Design before coding.** For feature development, plan around the requested functionality before implementation.
- **Reuse existing code.** Search for existing utilities before creating new ones. Prefer extending or adapting existing utilities over introducing duplicates.
- **Avoid over-abstraction.** Do not introduce unnecessary abstraction layers, engines, architectures, classes, or structs unless explicitly requested.
- **Tests by request only.** Do not create or run unit tests unless explicitly requested. Unit tests should generally be reserved for shared utilities and critical functions widely used across the codebase.

## Documentation

- **Read the project documentation** in `documents/` before making changes.
- Check `documents/version_logs/` for release history and planned changes. Treat a version log as current planning context if its version is newer than the latest Git tag, or if no newer Git tag exists.
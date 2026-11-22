# alma-gantt

Open-source Gantt and scheduling tool, built so that AI agents can do most of the planning work
and humans can review and edit everything by hand. The plan is a plain-text file you keep in git;
the engine explains every date it computes; exports print cleanly. No watermarks, ever.

**Status:** early development (milestone M0: engine, file format and agent CLI). Nothing is usable yet.

- Specification: [`SPEC.md`](SPEC.md)
- Decisions: [`docs/adr/`](docs/adr/)
- Project-management reference notes: [`docs/knowledge/`](docs/knowledge/)
- Interactive UI sketch: [`prototype/index.html`](prototype/index.html) (open in a browser)
- Example plan: [`examples/hybrid-terminal.alma.toml`](examples/hybrid-terminal.alma.toml)

## Build

```sh
cargo build --workspace
cargo test --workspace
cargo run -p alma-cli
```

The toolchain version is pinned in `rust-toolchain.toml`; `rustup` installs it automatically.

## License

Dual-licensed under [MIT](LICENSE-MIT) or [Apache-2.0](LICENSE-APACHE), at your option.

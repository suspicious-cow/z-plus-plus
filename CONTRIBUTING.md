# Contributing to z++

Thanks for taking a look. z++ is a small project, so the process is light.

## Reporting a bug or asking for a feature

Open an [issue](https://github.com/suspicious-cow/z-plus-plus/issues/new/choose). For bugs, include your Windows version, whether you run the ARM64 or x64 build, and the steps that cause the problem.

## Sending a change

1. Open an issue first for anything bigger than a small fix, so we can agree on the approach.
2. Build and test on Windows (see [Building](README.md#building)).
3. Before you push, run:

   ```
   cargo fmt --check
   cargo clippy --release -- -D warnings
   cargo test
   ```

4. Keep each pull request to one change, and add a line to [CHANGELOG.md](CHANGELOG.md) under Unreleased.

## Ground rules for the code

- z++ stays one small exe with no runtime dependencies. New crates need a good reason.
- Keep functions short and give constants names instead of bare numbers.
- Never change a file the user did not ask to change. If z++ cannot save a file back exactly as it read it, it should refuse to open it.

By contributing, you agree that your contributions are licensed under the [Apache License 2.0](LICENSE).

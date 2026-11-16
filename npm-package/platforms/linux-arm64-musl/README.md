# @polylint/adrlint-linux-arm64-musl

The prebuilt `adrlint` binary for **Linux arm64 (musl)** (`aarch64-unknown-linux-musl`).

This package exists only so that [`@polylint/adrlint`](https://www.npmjs.com/package/@polylint/adrlint)
can pull exactly one platform binary through `optionalDependencies`. Install that package instead; it
puts the `adrlint` command on your `PATH`:

```sh
npm i -g @polylint/adrlint
```

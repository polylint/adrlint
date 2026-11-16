# @polylint/adrlint-linux-arm64-gnu

The prebuilt `adrlint` binary for **Linux arm64 (glibc)** (`aarch64-unknown-linux-gnu`).

This package exists only so that [`@polylint/adrlint`](https://www.npmjs.com/package/@polylint/adrlint)
can pull exactly one platform binary through `optionalDependencies`. Install that package instead; it
puts the `adrlint` command on your `PATH`:

```sh
npm i -g @polylint/adrlint
```

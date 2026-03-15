# Matt's Wiki Project

MWP (Matt's Wiki Project) is a static site generator for a personal markdown wiki.

It renders the markdown tree into ready-to-serve HTML and builds a Pagefind search bundle from linked pages, so the final site can be hosted as plain static files.

## Development

`../mwp` is now a normal CLI. Run it with Cargo:

```sh
cargo run -p mwp -- --help
```

Or install it locally from the repo:

```sh
cargo install --path mwp-cli
```

Then use `mwp ...` directly.

### Build the static site

```sh
cargo run -p mwp -- build --root /path/to/wiki --output dist
```

The `build` command renders the wiki into `dist/`, writes shared assets, and generates the `dist/pagefind/` bundle in one run.
Remote pages are cached in `.mwp-cache/` by default, revalidated after 168 hours, and reused automatically on repeated builds.

Useful flags:

```sh
--cache-dir .mwp-cache
--cache-ttl-hours 168
--offline
```

### Build only the Pagefind search bundle

```sh
cargo run -p mwp -- index --root /path/to/wiki --output dist/pagefind
```

Example with cache controls:

```sh
cargo run -p mwp -- index --root /path/to/wiki --output dist/pagefind --cache-dir .mwp-cache --cache-ttl-hours 24
```

### Serve the built site locally

```sh
cargo run -p mwp -- serve --dir dist --addr 127.0.0.1:4444
```

### Build and preview the wiki in `../wiki`

From inside `../mwp`:

```sh
cargo run -p mwp -- build --root ../wiki --output dist
cargo run -p mwp -- serve --dir dist --addr 127.0.0.1:4444
```

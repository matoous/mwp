# Matt's Wiki Project

MWP (Matt's Wiki Project) is a personal knowledge base/wiki.

MWP aims to address my frustration with other tools and approaches I used for storing links, notes, and snippets. Notably, it has integrated search that allows searching in the content of linked websites, it can render markdown notes, and aims to have as little external dependencies as possible. Most importantly though, it's a fun little project for learning rust.

MWP is the facade for [matoous/wiki](https://github.com/matoous/wiki) hosted on [fly.io](https://fly.io).

## Development

### Web

```sh
cargo watch -i mwp-web/static/ -x run
```

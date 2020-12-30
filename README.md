# Zettel Utils

<img src="./Agnotestic.png" width="50" />

CLI/Library Utility for searching Zettelkastens

## Setup

Make sure you have [rust](https://rustup.rs) installed

```
git clone https://github.com/dustinknopoff/zettel-utils
cd zettel-utils
cargo build --release
```

`zettel-utils` expects a config.toml to be in the current path it's called from.

```toml
wiki-location = "/Users/john/wiki"
zettel-dataformat = "%Y%m%d%H%M%S"
```

To see all possible inputs for the dataformat see chrono's [documentation](https://docs.rs/chrono/0.4.19/chrono/format/strftime/index.html)

## Features

Results can be output in one of the following formats:
- stdout
- JSON
- [Alfred](https://www.alfredapp.com/help/workflows/inputs/script-filter/json/)

Currently only used as a CLI

### `create` subcommand

```
zettel-utils create
```

Creates a `zettel.db` in the current path, walking the `wiki-location` and adding content/metadata

### `update` subcommand

```
zettel-utils update -a| --all -p | --paths ...paths
```

Passing the `-a` flag will replace all paths in the database, re-walking the `wiki-location` and updating

Otherwise, the `--paths` option takes in a list of paths to update in the database (INSERT OR REPLACE)

### `full-text` subcommand

```
zettel-utils full-text <text>
```

Search the full text of your zettel 

### `links` subcommand

```
zettel-utils links <text>
```

Search all markdown links in your wiki that match the input 

Ideal for detecting backlinks

### `tags` subcommand

```
zettel-utils tags <text>
```

Search all markdown tags in your wiki that match the input 

###

```
zettel-utils 0.1

USAGE:
    zettel-utils [format] <SUBCOMMAND>

ARGS:
    <format>    One of stdout, alfred, or json [default: stdout]

FLAGS:
    -h, --help       Prints help information
    -V, --version    Prints version information

SUBCOMMANDS:
    create       Creates a database storing your metadata about your zettels
    full-text    Search all documents in your wiki
    help         Prints this message or the help of the given subcommand(s)
    links        Find backlinks
    tags         Find all zettels with matching tag
    update       Update all or some of the database
```